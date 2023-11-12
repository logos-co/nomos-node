pub mod api;
mod config;

use color_eyre::eyre::Result;
use full_replication::Certificate;
#[cfg(feature = "metrics")]
use metrics::{backend::map::MapMetricsBackend, types::MetricsData, MetricsService};
use nomos_core::{
    da::{blob, certificate},
    tx::Transaction,
};

use nomos_log::Logger;
use nomos_mempool::{Certificate as CertDiscriminant, Transaction as TxDiscriminant};
use nomos_network::backends::libp2p::Libp2p;
use nomos_node_api::{http::backend::axum::AxumBackend, ApiService};
pub use nomos_node_lib::tx::Tx;
use nomos_node_lib::{Carnot, DataAvailabilityService, Mempool, Wire};
use nomos_storage::{backends::sled::SledBackend, StorageService};

pub use config::{Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, NetworkArgs, OverlayArgs};

use nomos_network::NetworkService;
use nomos_system_sig::SystemSig;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;

pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";

#[derive(Services)]
pub struct Nomos {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Libp2p>>,
    cl_mempool: ServiceHandle<Mempool<Tx, <Tx as Transaction>::Hash, TxDiscriminant>>,
    da_mempool: ServiceHandle<
        Mempool<
            Certificate,
            <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
            CertDiscriminant,
        >,
    >,
    consensus: ServiceHandle<Carnot>,
    http: ServiceHandle<ApiService<AxumBackend>>,
    #[cfg(feature = "metrics")]
    metrics: ServiceHandle<MetricsService<MapMetricsBackend<MetricsData>>>,
    da: ServiceHandle<DataAvailabilityService>,
    storage: ServiceHandle<StorageService<SledBackend<Wire>>>,
    system_sig: ServiceHandle<SystemSig>,
}
