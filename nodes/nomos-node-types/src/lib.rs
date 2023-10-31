use bytes::Bytes;
use consensus_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use full_replication::{AbsoluteNumber, Attestation, Blob, Certificate, FullReplication};
use nomos_consensus::{
    network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter, CarnotConsensus,
};
use nomos_core::{
    da::{
        blob,
        certificate::{self, select::FillSize as FillSizeWithBlobsCertificate},
    },
    tx::{select::FillSize as FillSizeWithTx, Transaction},
    wire,
};
use nomos_da::{
    backend::memory_cache::BlobCache, network::adapters::libp2p::Libp2pAdapter as DaLibp2pAdapter,
    DataAvailabilityService,
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter,
    Certificate as CertDiscriminant, MempoolService, Transaction as TxDiscriminant,
};
use nomos_storage::backends::{sled::SledBackend, StorageSerde};

use serde::{de::DeserializeOwned, Serialize};

use tx::Tx;

pub mod tx;

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
    SledBackend<Wire>,
>;

pub type DataAvailability = DataAvailabilityService<
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
    BlobCache<<Blob as nomos_core::da::blob::Blob>::Hash, Blob>,
    DaLibp2pAdapter<Blob, Attestation>,
>;

pub type DaMempoolService = MempoolService<
    MempoolLibp2pAdapter<Certificate, <Blob as blob::Blob>::Hash>,
    MockPool<Certificate, <Blob as blob::Blob>::Hash>,
    CertDiscriminant,
>;

pub type ClMempoolService = MempoolService<
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    TxDiscriminant,
>;

pub type Mempool<K, V, D> = MempoolService<MempoolLibp2pAdapter<K, V>, MockPool<K, V>, D>;

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
