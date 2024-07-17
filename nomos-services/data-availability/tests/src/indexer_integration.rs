use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::Duration;

use crate::common::*;
use bytes::Bytes;
use cryptarchia_consensus::TimeConfig;
use cryptarchia_ledger::{Coin, LedgerState};
use full_replication::attestation::Attestation;
use full_replication::{Certificate, VidCertificate};
use nomos_core::da::certificate::metadata::Metadata;
use nomos_core::da::certificate::vid::VidCertificate as _;
use nomos_core::da::certificate::Certificate as _;
use nomos_core::da::certificate::CertificateStrategy;
use nomos_core::da::Signer;
use nomos_core::{da::certificate, tx::Transaction};
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_storage::fs::write_blob;
use nomos_libp2p::{Multiaddr, SwarmConfig};
use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;
use nomos_mempool::{DaMempoolSettings, TxMempoolSettings};
use nomos_network::backends::libp2p::{Libp2p as NetworkBackend, Libp2pConfig};
use nomos_network::{NetworkConfig, NetworkService};
use nomos_node::{Tx, Wire};
use nomos_storage::{backends::rocksdb::RocksBackend, StorageService};
use overwatch_derive::*;
use overwatch_rs::overwatch::{Overwatch, OverwatchRunner};
use overwatch_rs::services::handle::ServiceHandle;
use rand::{thread_rng, Rng};
use tempfile::{NamedTempFile, TempDir};
use time::OffsetDateTime;
use nomos_core::da::attestation::Attestation as TraitAttestation;

use blake2::{
    digest::{Update, VariableOutput},
    Blake2bVar,
};

#[derive(Services)]
struct IndexerNode {
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    cl_mempool: ServiceHandle<TxMempool>,
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
                    id: <Certificate as certificate::Certificate>::id,
                },
                verification_provider: full_replication::CertificateVerificationParameters {
                    threshold: 0,
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

    let mempool = node1.handle().relay::<DaMempool>();
    let storage = node1.handle().relay::<StorageService<RocksBackend<Wire>>>();
    let indexer = node1.handle().relay::<DaIndexer>();

    let blob_hash = [9u8; 32];
    let app_id = [7u8; 32];
    let index = 0.into();

    let attestation = Attestation::new_signed(blob_hash, ids[0], &MockKeyPair);
    let certificate_strategy = full_replication::AbsoluteNumber::new(1);
    let cert = certificate_strategy.build(vec![attestation.clone()], app_id, index);
    let cert_id = cert.id();
    let vid: VidCertificate = cert.clone().into();
    let range = 0.into()..1.into(); // get idx 0 and 1.

    // Test generate hash for Attestation
    let hash2 = attestation.hash();

    let mut hasher = Blake2bVar::new(32).unwrap();
    hasher.update([blob_hash, ids[0]].concat().as_ref());
    let mut output = [0; 32];
    hasher.finalize_variable(&mut output).unwrap();

    assert_eq!(hash2, output);

    // Test generate signature for Certificate
    let _sig = cert.signature();

    // Test get Metadata for Certificate
    let (app_id2, index2) = cert.metadata();

    assert_eq!(app_id, app_id2);
    assert_eq!(index, index2);

    // Mock attestation step where blob is persisted in nodes blob storage.
    let rt = tokio::runtime::Runtime::new().unwrap();
    rt.block_on(write_blob(
        blobs_dir,
        vid.certificate_id().as_ref(),
        b"blob",
    ))
    .unwrap();

    node1.spawn(async move {
        let mempool_outbound = mempool.connect().await.unwrap();
        let storage_outbound = storage.connect().await.unwrap();
        let indexer_outbound = indexer.connect().await.unwrap();

        // Mock attested blob by writting directly into the da storage.
        let mut attested_key = Vec::from(b"da/attested/" as &[u8]);
        attested_key.extend_from_slice(&blob_hash);

        storage_outbound
            .send(nomos_storage::StorageMsg::Store {
                key: attested_key.into(),
                value: Bytes::new(),
            })
            .await
            .unwrap();

        // Put cert into the mempool.
        let (mempool_tx, mempool_rx) = tokio::sync::oneshot::channel();
        mempool_outbound
            .send(nomos_mempool::MempoolMsg::Add {
                payload: cert,
                key: cert_id,
                reply_channel: mempool_tx,
            })
            .await
            .unwrap();
        let _ = mempool_rx.await.unwrap();

        // Wait for block in the network.
        tokio::time::sleep(Duration::from_secs(2)).await;

        // Request range of vids from indexer.
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

        // Since we've only attested to certificate at idx 0, the first
        // item should have "some" data, other indexes should be None.
        app_id_blobs.sort_by(|(a, _), (b, _)| a.partial_cmp(b).unwrap());
        let app_id_blobs = app_id_blobs.iter().map(|(_, b)| b).collect::<Vec<_>>();
        if let Some(blob) = app_id_blobs[0] {
            if **blob == *b"blob" && app_id_blobs[1].is_none() {
                is_success_tx.store(true, SeqCst);
            }
        }

        performed_tx.store(true, SeqCst);
    });

    while !performed_rx.load(SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(is_success_rx.load(SeqCst));

}

struct MockKeyPair;

impl Signer for MockKeyPair {
    fn sign(&self, _message: &[u8]) -> Vec<u8> {
        vec![]
    }
}
