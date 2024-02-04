pub mod api;
pub mod config;
mod tx;

use carnot_consensus::network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter;
use carnot_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use color_eyre::eyre::Result;
use full_replication::Certificate;
use full_replication::{AbsoluteNumber, Attestation, Blob, FullReplication};

use api::AxumBackend;
use bytes::Bytes;
use carnot_consensus::CarnotConsensus;
use nomos_api::ApiService;
use nomos_core::{
    da::{blob, certificate},
    tx::Transaction,
    wire,
};
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaLibp2pAdapter,
    DataAvailabilityService,
};
use nomos_log::Logger;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter;
use nomos_mempool::{
    backend::mockpool::MockPool, Certificate as CertDiscriminant, MempoolService,
    Transaction as TxDiscriminant,
};
#[cfg(feature = "metrics")]
use nomos_metrics::Metrics;
use nomos_network::backends::libp2p::Libp2p;
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageService,
};

pub use config::{
    Config, ConsensusArgs, DaArgs, HttpArgs, LogArgs, MetricsArgs, NetworkArgs, OverlayArgs,
};
use nomos_core::{
    da::certificate::select::FillSize as FillSizeWithBlobsCertificate,
    tx::select::FillSize as FillSizeWithTx,
};
use nomos_network::NetworkService;
use nomos_system_sig::SystemSig;
use overwatch_derive::*;
use overwatch_rs::services::handle::ServiceHandle;
use serde::{de::DeserializeOwned, Serialize};

pub use tx::Tx;

pub const CL_TOPIC: &str = "cl";
pub const DA_TOPIC: &str = "da";
const MB16: usize = 1024 * 1024 * 16;

pub type Carnot = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Certificate, <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash>,
    MempoolLibp2pAdapter<
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, Certificate>,
    RocksBackend<Wire>,
>;

pub type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaLibp2pAdapter<Blob, Attestation>,
>;

type Mempool<K, V, D> = MempoolService<MempoolLibp2pAdapter<K, V>, MockPool<K, V>, D>;

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
    http: ServiceHandle<ApiService<AxumBackend<Tx, Wire, MB16>>>,
    da: ServiceHandle<DataAvailability>,
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

pub fn run(
    config: Config,
    #[cfg(feature = "metrics")] metrics_args: MetricsArgs,
) -> color_eyre::Result<()> {
    use color_eyre::eyre::eyre;

    use nomos_mempool::network::adapters::libp2p::Settings as AdapterSettings;
    #[cfg(feature = "metrics")]
    use nomos_metrics::MetricsSettings;
    use overwatch_rs::overwatch::*;

    #[cfg(feature = "metrics")]
    let registry = cfg!(feature = "metrics")
        .then(|| {
            metrics_args
                .with_metrics
                .then(nomos_metrics::NomosRegistry::default)
        })
        .flatten();

    #[cfg(not(feature = "metrics"))]
    let registry = None;

    let app = OverwatchRunner::<Nomos>::run(
        NomosServiceSettings {
            network: config.network,
            logging: config.log,
            http: config.http,
            cl_mempool: nomos_mempool::Settings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(crate::CL_TOPIC),
                    id: <Tx as Transaction>::hash,
                },
                registry: registry.clone(),
            },
            da_mempool: nomos_mempool::Settings {
                backend: (),
                network: AdapterSettings {
                    topic: String::from(crate::DA_TOPIC),
                    id: cert_id,
                },
                registry: registry.clone(),
            },
            consensus: config.consensus,
            #[cfg(feature = "metrics")]
            metrics: MetricsSettings { registry },
            da: config.da,
            storage: config.storage,
            system_sig: (),
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}

fn cert_id(cert: &Certificate) -> <Blob as blob::Blob>::Hash {
    use certificate::Certificate;
    cert.hash()
}
