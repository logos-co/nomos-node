mod api;

// std
// crates
use kzgrs_backend::common::blob::DaBlob;
use nomos_api::ApiService;
use nomos_da_sampling::backend::kzgrs::KzgrsSamplingBackend;
use nomos_da_sampling::storage::adapters::rocksdb::RocksAdapter as SamplingStorageAdapter;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_node::*;
use overwatch_derive::Services;
use overwatch_rs::services::handle::ServiceHandle;
use rand_chacha::ChaCha20Rng;
// internal
use api::backend::AxumBackend;

pub type ExecutorApiService = ApiService<
    AxumBackend<
        (),
        DaBlob,
        BlobInfo,
        NomosDaMembership,
        BlobInfo,
        KzgrsDaVerifier,
        Tx,
        Wire,
        KzgrsSamplingBackend<ChaCha20Rng>,
        nomos_da_sampling::network::adapters::libp2p::Libp2pAdapter<NomosDaMembership>,
        ChaCha20Rng,
        SamplingStorageAdapter<DaBlob, Wire>,
        MB16,
    >,
>;

#[derive(Services)]
pub struct NomosExecutor {
    #[cfg(feature = "tracing")]
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    da_indexer: ServiceHandle<DaIndexer>,
    da_verifier: ServiceHandle<DaVerifier>,
    da_sampling: ServiceHandle<DaSampling>,
    da_network: ServiceHandle<DaNetworkService<DaNetworkValidatorBackend<NomosDaMembership>>>,
    cl_mempool: ServiceHandle<TxMempool>,
    da_mempool: ServiceHandle<DaMempool>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    http: ServiceHandle<NomosApiService>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<Metrics>,
    system_sig: ServiceHandle<SystemSig>,
}
