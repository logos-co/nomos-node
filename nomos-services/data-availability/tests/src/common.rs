use bytes::Bytes;
use full_replication::BlobInfo;
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::{da::blob::info::DispersedBlobInfo, header::HeaderId, tx::Transaction};
use nomos_da_indexer::consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter;
use nomos_da_indexer::DataIndexerService;
use nomos_da_verifier::backend::kzgrs::KzgrsDaVerifier;
use nomos_da_verifier::network::adapters::libp2p::Libp2pAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter;
use nomos_da_verifier::DaVerifierService;
use nomos_libp2p::{Multiaddr, Swarm, SwarmConfig};
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
use nomos_storage::backends::rocksdb::RocksBackend;

pub use nomos_core::{
    da::blob::select::FillSize as FillSizeWithBlobs, tx::select::FillSize as FillSizeWithTx,
};
use nomos_mempool::da::service::DaMempoolService;
use nomos_node::{Tx, Wire};

pub(crate) type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
>;

pub(crate) type DaIndexer = DataIndexerService<
    // Indexer specific.
    Bytes,
    IndexerStorageAdapter<Wire, full_replication::BlobInfo>,
    CryptarchiaConsensusAdapter<Tx, full_replication::BlobInfo>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobs<MB16, BlobInfo>,
    RocksBackend<Wire>,
>;

pub(crate) type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub(crate) type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as DispersedBlobInfo>::BlobId>,
>;

pub(crate) type DaVerifier = DaVerifierService<
    KzgrsDaVerifier,
    Libp2pAdapter<DaBlob, ()>,
    VerifierStorageAdapter<(), DaBlob, Wire>,
>;

pub(crate) const MB16: usize = 1024 * 1024 * 16;

pub fn node_address(config: &SwarmConfig) -> Multiaddr {
    Swarm::multiaddr(std::net::Ipv4Addr::new(127, 0, 0, 1), config.port)
}
