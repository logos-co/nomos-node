use cryptarchia_consensus::LeaderConfig;
// std
use nomos_blend::membership::Node;
use nomos_blend::message_blend::{
    CryptographicProcessorSettings, MessageBlendSettings, TemporalSchedulerSettings,
};
use nomos_blend_message::{sphinx::SphinxMessage, BlendMessage};
use nomos_da_network_service::backends::libp2p::common::DaNetworkBackendSettings;
use std::path::{Path, PathBuf};
use std::time::Duration;
// crates
use cryptarchia_consensus::TimeConfig;
use kzgrs_backend::common::blob::DaBlob;
use kzgrs_backend::dispersal::BlobInfo;
use kzgrs_backend::encoder::DaEncoder;
use kzgrs_backend::encoder::DaEncoderParams;
use libp2p::identity::{
    ed25519::{self, Keypair as Ed25519Keypair},
    Keypair, PeerId,
};
use nomos_blend_service::backends::libp2p::{
    Libp2pBlendBackend as BlendBackend, Libp2pBlendBackendSettings,
};
use nomos_blend_service::{BlendConfig, BlendService};
use nomos_core::{da::blob::info::DispersedBlobInfo, header::HeaderId, tx::Transaction};
pub use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
use nomos_da_indexer::consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapterSettings as IndexerStorageSettings;
use nomos_da_indexer::DataIndexerService;
use nomos_da_indexer::IndexerSettings;
use nomos_da_network_service::backends::libp2p::validator::DaNetworkValidatorBackend;
use nomos_da_network_service::NetworkConfig as DaNetworkConfig;
use nomos_da_network_service::NetworkService as DaNetworkService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackendSettings;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapterSettings as SamplingStorageSettings;
use nomos_da_sampling::DaSamplingService;
use nomos_da_sampling::DaSamplingServiceSettings;
use nomos_da_sampling::{
    backend::kzgrs::KzgrsSamplingBackend,
    network::adapters::validator::Libp2pAdapter as SamplingLibp2pAdapter,
};
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifierSettings;
use nomos_da_verifier::network::adapters::validator::Libp2pAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapterSettings as VerifierStorageSettings;
use nomos_da_verifier::DaVerifierService;
use nomos_da_verifier::DaVerifierServiceSettings;
use nomos_ledger::LedgerState;
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
use once_cell::sync::Lazy;
use overwatch_derive::*;
use overwatch_rs::overwatch::{Overwatch, OverwatchRunner};
use overwatch_rs::OpaqueServiceHandle;
use rand::{Rng, RngCore};
use subnetworks_assignations::versions::v1::FillFromNodeList;
// internal
use crate::rng::TestRng;

type IntegrationRng = TestRng;

/// Membership used by the DA Network service.
pub type NomosDaMembership = FillFromNodeList;

pub const GLOBAL_PARAMS_PATH: &str = "../../../tests/kzgrs/kzgrs_test_params";

pub const SK1: &str = "aca2c52f5928a53de79679daf390b0903eeccd9671b4350d49948d84334874806afe68536da9e076205a2af0af350e6c50851a040e3057b6544a29f5689ccd31";
pub const SK2: &str = "f9dc26eea8bc56d9a4c59841b438665b998ce5e42f49f832df5b770a725c2daafee53b33539127321f6f5085e42902bd380e82d18a7aff6404e632b842106785";

pub static PARAMS: Lazy<DaEncoderParams> = Lazy::new(|| {
    let global_parameters =
        kzgrs_backend::global::global_parameters_from_file(GLOBAL_PARAMS_PATH).unwrap();
    DaEncoderParams::new(2, false, global_parameters)
});
pub static ENCODER: Lazy<DaEncoder> = Lazy::new(|| DaEncoder::new(PARAMS.clone()));

pub(crate) type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapter<
        nomos_blend_service::network::libp2p::Libp2pAdapter,
        Tx,
        BlobInfo,
    >,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<IntegrationRng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    IntegrationRng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub type DaSampling = DaSamplingService<
    KzgrsSamplingBackend<IntegrationRng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    IntegrationRng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type DaIndexer = DataIndexerService<
    // Indexer specific.
    DaBlob,
    IndexerStorageAdapter<Wire, BlobInfo>,
    CryptarchiaConsensusAdapter<Tx, BlobInfo>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapter<
        nomos_blend_service::network::libp2p::Libp2pAdapter,
        Tx,
        BlobInfo,
    >,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
    KzgrsSamplingBackend<IntegrationRng>,
    SamplingLibp2pAdapter<NomosDaMembership>,
    IntegrationRng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    KzgrsSamplingBackend<IntegrationRng>,
    nomos_da_sampling::network::adapters::validator::Libp2pAdapter<NomosDaMembership>,
    IntegrationRng,
    SamplingStorageAdapter<DaBlob, Wire>,
>;

pub(crate) type DaVerifier = DaVerifierService<
    KzgrsDaVerifier,
    Libp2pAdapter<NomosDaMembership>,
    VerifierStorageAdapter<(), DaBlob, Wire>,
>;

pub(crate) const MB16: usize = 1024 * 1024 * 16;

#[derive(Services)]
pub struct TestNode {
    //logging: OpaqueServiceHandle<Logger>,
    network: OpaqueServiceHandle<NetworkService<NetworkBackend>>,
    blend: OpaqueServiceHandle<
        BlendService<BlendBackend, nomos_blend_service::network::libp2p::Libp2pAdapter>,
    >,
    cl_mempool: OpaqueServiceHandle<TxMempool>,
    da_network: OpaqueServiceHandle<DaNetworkService<DaNetworkValidatorBackend<FillFromNodeList>>>,
    da_mempool: OpaqueServiceHandle<DaMempool>,
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
    cryptarchia: OpaqueServiceHandle<Cryptarchia>,
    indexer: OpaqueServiceHandle<DaIndexer>,
    verifier: OpaqueServiceHandle<DaVerifier>,
    da_sampling: OpaqueServiceHandle<DaSampling>,
}

pub struct TestDaNetworkSettings {
    pub peer_addresses: Vec<(PeerId, Multiaddr)>,
    pub listening_address: Multiaddr,
    pub num_subnets: u16,
    pub num_samples: u16,
    pub nodes_per_subnet: u16,
    pub node_key: ed25519::SecretKey,
}

pub struct TestBlendSettings {
    pub backend: Libp2pBlendBackendSettings,
    pub private_key: x25519_dalek::StaticSecret,
    pub membership: Vec<
        Node<
            <BlendBackend as nomos_blend_service::backends::BlendBackend>::NodeId,
            <SphinxMessage as BlendMessage>::PublicKey,
        >,
    >,
}

#[allow(clippy::too_many_arguments)]
pub fn new_node(
    leader_config: &LeaderConfig,
    ledger_config: &nomos_ledger::Config,
    genesis_state: &LedgerState,
    time_config: &TimeConfig,
    swarm_config: &SwarmConfig,
    blend_config: &TestBlendSettings,
    db_path: PathBuf,
    blobs_dir: &Path,
    initial_peers: Vec<Multiaddr>,
    verifier_settings: KzgrsDaVerifierSettings,
    da_network_settings: TestDaNetworkSettings,
) -> Overwatch {
    OverwatchRunner::<TestNode>::run(
        TestNodeServiceSettings {
            //logging: Default::default(),
            network: NetworkConfig {
                backend: Libp2pConfig {
                    inner: swarm_config.clone(),
                    initial_peers,
                },
            },
            blend: BlendConfig {
                backend: blend_config.backend.clone(),
                persistent_transmission: Default::default(),
                message_blend: MessageBlendSettings {
                    cryptographic_processor: CryptographicProcessorSettings {
                        private_key: blend_config.private_key.to_bytes(),
                        num_blend_layers: 1,
                    },
                    temporal_processor: TemporalSchedulerSettings {
                        max_delay_seconds: 2,
                    },
                },
                cover_traffic: nomos_blend_service::CoverTrafficExtSettings {
                    epoch_duration: Duration::from_secs(432000),
                    slot_duration: Duration::from_secs(20),
                },
                membership: blend_config.membership.clone(),
            },
            da_network: DaNetworkConfig {
                backend: DaNetworkBackendSettings {
                    node_key: da_network_settings.node_key,
                    membership: FillFromNodeList::new(
                        &da_network_settings
                            .peer_addresses
                            .iter()
                            .map(|(peer_id, _)| *peer_id)
                            .collect::<Vec<PeerId>>(),
                        da_network_settings.num_subnets.into(),
                        da_network_settings.nodes_per_subnet.into(),
                    ),
                    addresses: da_network_settings.peer_addresses.into_iter().collect(),
                    listening_address: da_network_settings.listening_address,
                },
            },
            cl_mempool: TxMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
            },
            da_mempool: DaMempoolSettings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(nomos_node::DA_TOPIC),
                    id: <BlobInfo as DispersedBlobInfo>::blob_id,
                },
            },
            storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
                db_path,
                read_only: false,
                column_family: Some("blocks".into()),
            },
            indexer: IndexerSettings {
                storage: IndexerStorageSettings {
                    blob_storage_directory: blobs_dir.to_path_buf(),
                },
            },
            cryptarchia: cryptarchia_consensus::CryptarchiaSettings {
                transaction_selector_settings: (),
                blob_selector_settings: (),
                config: ledger_config.clone(),
                genesis_state: genesis_state.clone(),
                time: time_config.clone(),
                leader_config: leader_config.clone(),
                network_adapter_settings:
                    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapterSettings {
                        topic: String::from(nomos_node::CONSENSUS_TOPIC),
                    },
                blend_adapter_settings:
                    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapterSettings {
                        broadcast_settings:
                            nomos_blend_service::network::libp2p::Libp2pBroadcastSettings {
                                topic: String::from(nomos_node::CONSENSUS_TOPIC),
                            },
                    },
                recovery_file: PathBuf::from("./recovery/cryptarchia.json"),
            },
            verifier: DaVerifierServiceSettings {
                verifier_settings,
                network_adapter_settings: (),
                storage_adapter_settings: VerifierStorageSettings {
                    blob_storage_directory: blobs_dir.to_path_buf(),
                },
            },
            da_sampling: DaSamplingServiceSettings {
                // TODO: setup this properly!
                sampling_settings: KzgrsSamplingBackendSettings {
                    num_samples: da_network_settings.num_samples,
                    num_subnets: da_network_settings.num_subnets,
                    // Sampling service period can't be zero.
                    old_blobs_check_interval: Duration::from_secs(5),
                    blobs_validity_duration: Duration::from_secs(u64::MAX),
                },
                network_adapter_settings: (),
                storage_adapter_settings: SamplingStorageSettings {
                    blob_storage_directory: blobs_dir.to_path_buf(),
                },
            },
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap()
}

pub fn new_blend_configs(listening_addresses: Vec<Multiaddr>) -> Vec<TestBlendSettings> {
    let settings = listening_addresses
        .iter()
        .map(|listening_address| {
            (
                Libp2pBlendBackendSettings {
                    listening_address: listening_address.clone(),
                    node_key: ed25519::SecretKey::generate(),
                    peering_degree: 1,
                    max_peering_degree: 1,
                    conn_monitor: None,
                },
                x25519_dalek::StaticSecret::random(),
            )
        })
        .collect::<Vec<_>>();

    let membership = settings
        .iter()
        .map(|(backend, private_key)| Node {
            id: PeerId::from_public_key(
                &Keypair::from(Ed25519Keypair::from(backend.node_key.clone())).public(),
            ),
            address: backend.listening_address.clone(),
            public_key: x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
                private_key.to_bytes(),
            ))
            .to_bytes(),
        })
        .collect::<Vec<_>>();

    settings
        .into_iter()
        .map(|(backend, private_key)| TestBlendSettings {
            backend,
            private_key,
            membership: membership.clone(),
        })
        .collect()
}

// Client node is only created for asyncroniously interact with nodes in the test.
// The services defined in it are not used.
#[derive(Services)]
pub struct TestClient {
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
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

pub fn node_address(config: &SwarmConfig) -> Multiaddr {
    Swarm::multiaddr(std::net::Ipv4Addr::new(127, 0, 0, 1), config.port)
}

pub fn generate_blst_hex_keys() -> (String, String) {
    let mut rng = rand::thread_rng();
    let sk_bytes: [u8; 32] = rng.gen();
    let sk = blst::min_sig::SecretKey::key_gen(&sk_bytes, &[]).unwrap();

    let pk = sk.sk_to_pk();
    (hex::encode(sk.to_bytes()), hex::encode(pk.to_bytes()))
}

pub fn create_ed25519_sk_peerid(key: &str) -> (ed25519::SecretKey, PeerId) {
    let mut b = hex::decode(key).unwrap();
    let ed25519_keypair = Ed25519Keypair::try_from_bytes(&mut b).unwrap();
    let kp = ed25519_keypair.to_bytes();
    println!("sk > {}", hex::encode(kp));
    let secret_key = ed25519_keypair.secret().clone();
    let libp2p_keypair: Keypair = ed25519_keypair.into();
    let peer_id = PeerId::from_public_key(&libp2p_keypair.public());

    (secret_key, peer_id)
}

pub fn generate_ed25519_sk_peerid() -> (ed25519::SecretKey, PeerId) {
    let ed25519_keypair = Ed25519Keypair::generate();
    let kp = ed25519_keypair.to_bytes();
    println!("sk > {}", hex::encode(kp));
    let secret_key = ed25519_keypair.secret().clone();
    let libp2p_keypair: Keypair = ed25519_keypair.into();
    let peer_id = PeerId::from_public_key(&libp2p_keypair.public());

    (secret_key, peer_id)
}

pub fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}
