// std
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::atomic::Ordering::SeqCst;
use std::sync::Arc;
use std::time::Duration;
// crates
use cryptarchia_consensus::TimeConfig;
use cryptarchia_ledger::{Coin, LedgerState};
use full_replication::BlobInfo;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::encoder::{DaEncoder, DaEncoderParams};
use nomos_core::da::{blob::info::DispersedBlobInfo, DaEncoder as _};
use nomos_core::tx::Transaction;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageSettings;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::validator::{
    DaNetworkValidatorBackend, DaNetworkValidatorBackendSettings,
};
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::network::adapters::libp2p::DaNetworkSamplingSettings;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageSettings;
use nomos_da_verifier::DaVerifierServiceSettings;
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
use overwatch_rs::services::handle::ServiceHandle;
use rand::{thread_rng, Rng, RngCore};
use subnetworks_assignations::versions::v1::FillFromNodeList;
use tempfile::{NamedTempFile, TempDir};
use time::OffsetDateTime;
// internal
use crate::common::*;

// Client node is only created for asyncroniously interact with nodes in the test.
// The services defined in it are not used.
#[derive(Services)]
struct ClientNode {
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
}

#[derive(Services)]
struct VerifierNode {
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<FillFromNodeList>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    indexer: ServiceHandle<DaIndexer>,
    verifier: ServiceHandle<DaVerifier>,
    da_sampling: ServiceHandle<DaSampling>,
}

// Client node is just an empty overwatch service to spawn a task that could communicate with other
// nodes and manage the data availability cycle during tests.
fn new_client(db_path: PathBuf) -> Overwatch {
    OverwatchRunner::<ClientNode>::run(
        ClientNodeServiceSettings {
            storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
                db_path,
                read_only: false,
                column_family: None,
            },
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap()
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
    verifier_settings: KzgrsDaVerifierSettings,
) -> Overwatch {
    OverwatchRunner::<VerifierNode>::run(
        VerifierNodeServiceSettings {
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
                storage: IndexerStorageSettings {
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
            verifier: DaVerifierServiceSettings {
                verifier_settings,
                network_adapter_settings: (),
                storage_adapter_settings: VerifierStorageSettings {
                    blob_storage_directory: blobs_dir.clone(),
                },
            },
            da_sampling: DaSamplingServiceSettings {
                // TODO: setup this properly!
                sampling_settings: KzgrsSamplingBackendSettings {
                    num_samples: 0,
                    // Sampling service period can't be zero.
                    old_blobs_check_interval: Duration::from_secs(1),
                    blobs_validity_duration: Duration::from_secs(1),
                },
                network_adapter_settings: DaNetworkSamplingSettings {
                    num_samples: 0,
                    subnet_size: 0,
                },
            },
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap()
}

fn generate_hex_keys() -> (String, String) {
    let mut rng = rand::thread_rng();
    let sk_bytes: [u8; 32] = rng.gen();
    let sk = blst::min_sig::SecretKey::key_gen(&sk_bytes, &[]).unwrap();

    let pk = sk.sk_to_pk();
    (hex::encode(sk.to_bytes()), hex::encode(pk.to_bytes()))
}

pub fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}

pub const PARAMS: DaEncoderParams = DaEncoderParams::default_with(2);
pub const ENCODER: DaEncoder = DaEncoder::new(PARAMS);

#[test]
fn test_verifier() {
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
        port: 7773,
        ..Default::default()
    };
    let swarm_config2 = SwarmConfig {
        port: 7774,
        ..Default::default()
    };

    let blobs_dir = TempDir::new().unwrap().path().to_path_buf();

    let (node1_sk, node1_pk) = generate_hex_keys();
    let (node2_sk, node2_pk) = generate_hex_keys();

    let client_zone = new_client(NamedTempFile::new().unwrap().path().to_path_buf());

    let node1 = new_node(
        &coins[0],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config1,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config2)],
        KzgrsDaVerifierSettings {
            sk: node1_sk.clone(),
            nodes_public_keys: vec![node1_pk.clone(), node2_pk.clone()],
        },
    );

    let node2 = new_node(
        &coins[1],
        &ledger_config,
        &genesis_state,
        &time_config,
        &swarm_config2,
        NamedTempFile::new().unwrap().path().to_path_buf(),
        &blobs_dir,
        vec![node_address(&swarm_config1)],
        KzgrsDaVerifierSettings {
            sk: node2_sk,
            nodes_public_keys: vec![node1_pk, node2_pk],
        },
    );

    let node1_verifier = node1.handle().relay::<DaVerifier>();

    let node2_verifier = node2.handle().relay::<DaVerifier>();

    client_zone.spawn(async move {
        let node1_verifier = node1_verifier.connect().await.unwrap();
        let (node1_reply_tx, node1_reply_rx) = tokio::sync::oneshot::channel();

        let node2_verifier = node2_verifier.connect().await.unwrap();
        let (node2_reply_tx, node2_reply_rx) = tokio::sync::oneshot::channel();

        let verifiers = vec![
            (node1_verifier, node1_reply_tx),
            (node2_verifier, node2_reply_tx),
        ];

        // Encode data
        let encoder = &ENCODER;
        let data = rand_data(10);

        let encoded_data = encoder.encode(&data).unwrap();
        let columns: Vec<_> = encoded_data.extended_data.columns().collect();

        for (i, (verifier, reply_tx)) in verifiers.into_iter().enumerate() {
            let column = &columns[i];

            let da_blob = DaBlob {
                column: column.clone(),
                column_idx: i
                    .try_into()
                    .expect("Column index shouldn't overflow the target type"),
                column_commitment: encoded_data.column_commitments[i],
                aggregated_column_commitment: encoded_data.aggregated_column_commitment,
                aggregated_column_proof: encoded_data.aggregated_column_proofs[i],
                rows_commitments: encoded_data.row_commitments.clone(),
                rows_proofs: encoded_data
                    .rows_proofs
                    .iter()
                    .map(|proofs| proofs.get(i).cloned().unwrap())
                    .collect(),
            };

            verifier
                .send(nomos_da_verifier::DaVerifierMsg::AddBlob {
                    blob: da_blob,
                    reply_channel: reply_tx,
                })
                .await
                .unwrap();
        }

        // Wait for response from the verifier.
        let a1 = node1_reply_rx.await.unwrap();
        let a2 = node2_reply_rx.await.unwrap();

        if a1.is_some() && a2.is_some() {
            is_success_tx.store(true, SeqCst);
        }

        performed_tx.store(true, SeqCst);
    });

    while !performed_rx.load(SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
    assert!(is_success_rx.load(SeqCst));
}
