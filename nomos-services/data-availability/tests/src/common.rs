// stad
use std::path::PathBuf;
use std::time::Duration;
// crates
use bytes::Bytes;
use cryptarchia_consensus::TimeConfig;
use cryptarchia_ledger::Coin;
use cryptarchia_ledger::LedgerState;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::dispersal::BlobInfo;
use kzgrs_backend::encoder::DaEncoderParams;
use nomos_core::{da::blob::info::DispersedBlobInfo, header::HeaderId, tx::Transaction};
pub use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
use nomos_da_indexer::consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageSettings;
use nomos_da_indexer::DataIndexerService;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::validator::{
    DaNetworkValidatorBackend, DaNetworkValidatorBackendSettings,
};
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::network::adapters::libp2p::DaNetworkSamplingSettings;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter;
use nomos_da_sampling::DaSamplingService;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_sampling::{
    backend::kzgrs::KzgrsSamplingBackend,
    network::adapters::libp2p::Libp2pAdapter as SamplingLibp2pAdapter,
};
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_da_verifier::network::adapters::libp2p::Libp2pAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageSettings;
use nomos_da_verifier::DaVerifierService;
use nomos_da_verifier::DaVerifierServiceSettings;
use nomos_libp2p::{ed25519, identity, PeerId};
use nomos_libp2p::{Multiaddr, Swarm, SwarmConfig};
use nomos_mempool::da::service::DaMempoolService;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
use nomos_mempool::{DaMempoolSettings, TxMempoolSettings};
use nomos_network::backends::libp2p::{Libp2p as NetworkBackend, Libp2pConfig};
use nomos_network::NetworkConfig;
use nomos_network::NetworkService;
use nomos_node::{Tx, Wire};
use nomos_storage::backends::rocksdb::RocksBackend;
use nomos_storage::StorageService;
use overwatch_derive::*;
use overwatch_rs::overwatch::{Overwatch, OverwatchRunner};
use overwatch_rs::services::handle::ServiceHandle;
use rand::{Rng, RngCore};
use rand_chacha::ChaCha20Rng;
use subnetworks_assignations::versions::v1::FillFromNodeList;
// internal

/// Membership used by the DA Network service.
pub type NomosDaMembership = FillFromNodeList;

pub(crate) type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type DaSampling = DaSamplingService<
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type DaIndexer = DataIndexerService<
    // Indexer specific.
    Bytes,
    IndexerStorageAdapter<Wire, BlobInfo>,
    CryptarchiaConsensusAdapter<Tx, BlobInfo>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<ChaCha20Rng>,
    nomos_da_sampling::network::adapters::libp2p::Libp2pAdapter<NomosDaMembership>,
    ChaCha20Rng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type DaVerifier = DaVerifierService<
    KzgrsDaVerifier,
    Libp2pAdapter<NomosDaMembership>,
    VerifierStorageAdapter<(), DaBlob, Wire>,
>;

pub(crate) const MB16: usize = 1024 * 1024 * 16;

pub fn node_address(config: &SwarmConfig) -> Multiaddr {
    Swarm::multiaddr(std::net::Ipv4Addr::new(127, 0, 0, 1), config.port)
}

pub fn generate_hex_keys() -> (String, String) {
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

#[derive(Services)]
pub struct TestNode {
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<FillFromNodeList>>>,
    da_mempool: ServiceHandle<DaMempool>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    indexer: ServiceHandle<DaIndexer>,
    verifier: ServiceHandle<DaVerifier>,
    da_sampling: ServiceHandle<DaSampling>,
}

pub fn new_node(
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
    OverwatchRunner::<TestNode>::run(
        TestNodeServiceSettings {
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

// Client node is only created for asyncroniously interact with nodes in the test.
// The services defined in it are not used.
#[derive(Services)]
pub struct TestClient {
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
}

// Client node is just an empty overwatch service to spawn a task that could communicate with other
// nodes and manage the data availability cycle during tests.
pub fn new_client(db_path: PathBuf) -> Overwatch {
    OverwatchRunner::<TestClient>::run(
        TestClientServiceSettings {
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
