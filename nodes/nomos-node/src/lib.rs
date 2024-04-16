pub mod api;
mod config;
mod tx;

use color_eyre::eyre::Result;
use full_replication::Certificate;
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};

use api::AxumBackend;
use bytes::Bytes;
pub use config::{Config, CryptarchiaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs};
use nomos_api::ApiService;
use nomos_core::da::certificate::mock::MockKeyStore;
use nomos_core::da::certificate::verify::DaCertificateVerifier;
use nomos_core::tx::mock::MockTxVerifier;
use nomos_core::{da::certificate, header::HeaderId, tx::Transaction, wire};
use nomos_da::auth::mock::MockDaAuth;
use nomos_log::Logger;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
#[cfg(feature = "metrics")]
use nomos_metrics::Metrics;
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageService,
};

#[cfg(feature = "carnot")]
use carnot_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
pub use nomos_core::{
    da::certificate::select::FillSize as FillSizeWithBlobsCertificate,
    tx::select::FillSize as FillSizeWithTx,
};
use nomos_mempool::da::service::DaMempoolService;
use nomos_network::NetworkService;
use nomos_system_sig::SystemSig;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;
use serde::{de::DeserializeOwned, Serialize};

pub use tx::Tx;

pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
const MB16: usize = 1024 * 1024 * 16;

pub type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, Certificate>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockTxVerifier,
    MockPool<HeaderId, Certificate, <Certificate as certificate::Certificate>::Id>,
    MempoolNetworkAdapter<Certificate, <Certificate as certificate::Certificate>::Id>,
    DaCertificateVerifier<[u8; 32], MockKeyStore<MockDaAuth>, Certificate>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, Certificate>,
    RocksBackend<Wire>,
>;

pub type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    cl_mempool: ServiceHandle<TxMempool>,
    cryptarchia: ServiceHandle<Cryptarchia>,
    http: ServiceHandle<ApiService<AxumBackend<Tx, Wire, MB16>>>,
    storage: ServiceHandle<StorageService<RocksBackend<Wire>>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<Metrics>,
    system_sig: ServiceHandle<SystemSig>,
}

pub struct Wire;

impl StorageSerde for Wire {
    type Error = wire::Error;

    fn serialize<T: Serialize>(value: T) -> Bytes {
        wire::serialize(&value).unwrap().into()
    }

    fn deserialize<T: DeserializeOwned>(buff: Bytes) -> Result<T, Self::Error> {
        wire::deserialize(&buff)
    }
}
