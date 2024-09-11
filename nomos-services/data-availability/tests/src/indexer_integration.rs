// std
use std::hash::{DefaultHasher, Hash};
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::Duration;
// crates
use bytes::Bytes;
use cryptarchia_consensus::ConsensusMsg;
use cryptarchia_consensus::TimeConfig;
use cryptarchia_ledger::{Coin, LedgerState};
use full_replication::{BlobInfo, Metadata};
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::metadata::Metadata as _;
use nomos_core::tx::Transaction;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::validator::{
    DaNetworkValidatorBackend, DaNetworkValidatorBackendSettings,
};
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_storage::fs::write_blob;
use nomos_da_storage::rocksdb::DA_VERIFIED_KEY_PREFIX;
use nomos_libp2p::{ed25519, identity, PeerId};
use nomos_libp2p::{Multiaddr, SwarmConfig};
use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;
use nomos_mempool::{DaMempoolSettings, TxMempoolSettings};
use nomos_network::backends::libp2p::{Libp2p as NetworkBackend, Libp2pConfig};
use nomos_network::{NetworkConfig, NetworkService};
use nomos_node::{Tx, Wire};
use nomos_storage::{backends::rocksdb::RocksBackend, StorageService};
use overwatch_derive::*;
use overwatch_rs::overwatch::{Overwatch, OverwatchRunner};
use overwatch_rs::services::{handle::ServiceHandle, ServiceData};
use rand::{thread_rng, Rng};
use subnetworks_assignations::versions::v1::FillFromNodeList;
use tempfile::{NamedTempFile, TempDir};
use time::OffsetDateTime;
use tokio_stream::{wrappers::BroadcastStream, StreamExt};
// internal
use crate::common::*;
use crate::service_wrapper::ServiceWrapper;

#[derive(Services)]
struct IndexerNode {
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<FillFromNodeList>>>,
    da_mempool: ServiceHandle<DaMempool>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    indexer: ServiceHandle<DaIndexer>,
}

fn new_node(
    coin: &Coin,
    ledger_config: &cryptarchia_ledger::Config,
    genesis_state: &LedgerState,
    time_config: &TimeConfig,
    swarm_config: &SwarmConfig,
    db_path: PathBuf,
    blobs_dir: &PathBuf,
    initial_peers: Vec<Multiaddr>,
) -> Overwatch {
    OverwatchRunner::<IndexerNode>::run(
        IndexerNodeServiceSettings {
            network: NetworkConfig {
                backend: Libp2pConfig {
                    inner: swarm_config.clone(),
                    initial_peers,
                },
            },
            da_network: DaNetworkConfig {
                backend: DaNetworkValidatorBackendSettings {
                    node_key: ed25519::SecretKey::generate(),
                    membership: FillFromNodeList::new(
                        &[PeerId::from(identity::Keypair::generate_ed25519().public())],
                        2,
                        1,
                    ),
                    addresses: Default::default(),
                    listening_address: "/ip4/127.0.0.1/udp/0/quic-v1".parse::<Multiaddr>().unwrap(),
                },
            },
            cl_mempool: TxMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
                registry: None,
            },
            da_mempool: DaMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::DA_TOPIC),
                    id: <BlobInfo as DispersedBlobInfo>::blob_id,
                },
                registry: None,
            },
            storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
                db_path,
                read_only: false,
                column_family: Some("blocks".into()),
            },
            indexer: IndexerSettings {
                storage: RocksAdapterSettings {
                    blob_storage_directory: blobs_dir.clone(),
                },
            },
            cryptarchia: cryptarchia_consensus::CryptarchiaSettings {
                transaction_selector_settings: (),
                blob_selector_settings: (),
                config: ledger_config.clone(),
                genesis_state: genesis_state.clone(),
                time: time_config.clone(),
                coins: vec![coin.clone()],
            },
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap()
}

// TODO: When verifier is implemented this test should be removed and a new one
// performed in integration tests crate using the real node.

#[ignore = "Membership needs to be configured correctly"]
#[test]
fn test_indexer() {
    let performed_tx = Arc::new(AtomicBool::new(false));
    let performed_rx = performed_tx.clone();
    let is_success_tx = Arc::new(AtomicBool::new(false));
    let is_success_rx = is_success_tx.clone();

    let mut ids = vec![[0; 32]; 2];
    for id in &mut ids {
        thread_rng().fill(id);
    }

    let coins = ids
        .iter()
        .map(|&id| Coin::new(id, id.into(), 1.into()))
        .collect::<Vec<_>>();
    let genesis_state = LedgerState::from_commitments(
        coins.iter().map(|c| c.commitment()),
        (ids.len() as u32).into(),
    );
    let ledger_config = cryptarchia_ledger::Config {
        epoch_stake_distribution_stabilization: 3,
        epoch_period_nonce_buffer: 3,
        epoch_period_nonce_stabilization: 4,
        consensus_config: cryptarchia_engine::Config {
            security_param: 10,
            active_slot_coeff: 0.9,
        },
    };
    let time_config = TimeConfig {
        slot_duration: Duration::from_secs(1),
        chain_start_time: OffsetDateTime::now_utc(),
    };

    let swarm_config1 = SwarmConfig {
        port: 7771,
        ..Default::default()
    };
    let swarm_config2 = SwarmConfig {
        port: 7772,
        ..Default::default()
    };

    let blobs_dir = TempDir::new().unwrap().path().to_path_buf();

    let node1 = new_node(
        &coins[0],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config1,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config2)],
    );

    let _node2 = new_node(
        &coins[1],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config2,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config1)],
    );

    let wrapper1 = ServiceWrapper::new(node1.handle());

    let mempool = node1.handle().relay::<DaMempool>();
    let storage = node1.handle().relay::<StorageService<RocksBackend<Wire>>>();
    let indexer = node1.handle().relay::<DaIndexer>();
    let consensus = node1.handle().relay::<Cryptarchia>();

    let blob_hash = [9u8; 32];
    let app_id = [7u8; 32];
    let index = 0.into();

    let range = 0.into()..1.into(); // get idx 0 and 1.
    let meta = Metadata::new(app_id, index);
    let blob_info = BlobInfo::new(blob_hash, meta);

    // Test get Metadata for Certificate
    let (app_id2, index2) = blob_info.metadata();

    assert_eq!(app_id2, app_id);
    assert_eq!(index2, index);

    // Test generate hash for Certificate with default Hasher
    let mut default_hasher = DefaultHasher::new();
    let _hash3 = <BlobInfo as Hash>::hash(&blob_info, &mut default_hasher);

    let expected_blob_info = blob_info.clone();
    let col_idx = (0 as u16).to_be_bytes();

    // Mock attestation step where blob is persisted in nodes blob storage.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(write_blob(
        blobs_dir,
        blob_info.blob_id().as_ref(),
        &col_idx,
        b"blob",
    ))
    .unwrap();

    node1.spawn(async move {
        let mempool_outbound = mempool.connect().await.unwrap();
        let storage_outbound = storage.connect().await.unwrap();
        let indexer_outbound = indexer.connect().await.unwrap();
        let consensus_outbound = consensus.connect().await.unwrap();

        wrapper1.wrap(mempool_outbound);
        wrapper1.wrap(&storage_outbound);
        wrapper1.wrap(&indexer_outbound);
        wrapper1.wrap(&consensus_outbound);

        let (sender, receiver) = tokio::sync::oneshot::channel();
        consensus_outbound
            .send(ConsensusMsg::BlockSubscribe { sender })
            .await
            .unwrap();
        let broadcast_receiver = receiver.await.unwrap();
        let mut broadcast_receiver =
            BroadcastStream::new(broadcast_receiver).filter_map(|result| match result {
                Ok(block) => Some(block),
                Err(_) => None,
            });

        // Mock attested blob by writting directly into the da storage.
        let mut attested_key = Vec::from(DA_VERIFIED_KEY_PREFIX.as_bytes());
        attested_key.extend_from_slice(&blob_hash);
        attested_key.extend_from_slice(&col_idx);

        storage_outbound
            .send(nomos_storage::StorageMsg::Store {
                key: attested_key.into(),
                value: Bytes::new(),
            })
            .await
            .unwrap();

        // Put blob_info into the mempool.
        let (mempool_tx, mempool_rx) = tokio::sync::oneshot::channel();
        mempool_outbound
            .send(nomos_mempool::MempoolMsg::Add {
                payload: blob_info,
                key: blob_hash,
                reply_channel: mempool_tx,
            })
            .await
            .unwrap();
        let _ = mempool_rx.await.unwrap();

        // Wait for block in the network.
        let timeout = tokio::time::sleep(Duration::from_secs(10));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Some(block) = broadcast_receiver.next() => {
                    if block.blobs().any(|b| *b == expected_blob_info) {
                        break;
                    }
                }
                _ = &mut timeout => {
                    break;
                }
            }
        }

        // Give time for services to process and store data.
        tokio::time::sleep(Duration::from_secs(1)).await;

        // Request range of blobs from indexer.
        let (indexer_tx, indexer_rx) = tokio::sync::oneshot::channel();
        indexer_outbound
            .send(nomos_da_indexer::DaMsg::GetRange {
                app_id,
                range,
                reply_channel: indexer_tx,
            })
            .await
            .unwrap();
        let mut app_id_blobs = indexer_rx.await.unwrap();

        // Since we've only attested to blob_info at idx 0, the first
        // item should have "some" data, other indexes should be None.
        app_id_blobs.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
        let app_id_blobs = app_id_blobs.iter().map(|(_, b)| b).collect::<Vec<_>>();

        // When Indexer is asked for app_id at index, it will return all blobs that it has for that
        // blob_id.
        let columns = app_id_blobs[0];
        if !columns.is_empty() && *columns[0] == *b"blob" && app_id_blobs[1].is_empty() {
            is_success_tx.store(true, SeqCst);
        }

        performed_tx.store(true, SeqCst);
    });

    while !performed_rx.load(SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(is_success_rx.load(SeqCst));
}
