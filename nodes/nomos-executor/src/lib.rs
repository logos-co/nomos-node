use nomos_node::*;
use overwatch_derive::Services;
use overwatch_rs::services::handle::ServiceHandle;

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
