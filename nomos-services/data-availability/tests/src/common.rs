use bytes::Bytes;
use full_replication::{Certificate, VidCertificate};
use nomos_core::{da::certificate, header::HeaderId, tx::Transaction};
use nomos_da_indexer::consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter;
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter;
use nomos_da_indexer::DataIndexerService;
use nomos_mempool::da::verify::fullreplication::DaVerificationProvider as MempoolVerificationProvider;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_mempool::{backend::mockpool::MockPool, TxMempoolService};
use nomos_storage::backends::rocksdb::RocksBackend;

pub use nomos_core::{
    da::certificate::select::FillSize as FillSizeWithBlobsCertificate,
    tx::select::FillSize as FillSizeWithTx,
};
use nomos_mempool::da::service::DaMempoolService;
use nomos_node::{Tx, Wire};

pub(crate) type Cryptarchia = cryptarchia_consensus::CryptarchiaConsensus<
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, VidCertificate>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<
        HeaderId,
        VidCertificate,
        <VidCertificate as certificate::vid::VidCertificate>::CertificateId,
    >,
    MempoolNetworkAdapter<Certificate, <Certificate as certificate::Certificate>::Id>,
    MempoolVerificationProvider,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, VidCertificate>,
    RocksBackend<Wire>,
>;

pub(crate) type DaIndexer = DataIndexerService<
    // Indexer specific.
    Bytes,
    RocksAdapter<Wire, full_replication::VidCertificate>,
    CryptarchiaConsensusAdapter<Tx, full_replication::VidCertificate>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, VidCertificate>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<
        HeaderId,
        VidCertificate,
        <VidCertificate as certificate::vid::VidCertificate>::CertificateId,
    >,
    MempoolNetworkAdapter<Certificate, <Certificate as certificate::Certificate>::Id>,
    MempoolVerificationProvider,
    FillSizeWithTx<MB16, Tx>,
    FillSizeWithBlobsCertificate<MB16, VidCertificate>,
    RocksBackend<Wire>,
>;

pub(crate) type TxMempool = TxMempoolService<
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
>;

pub(crate) type DaMempool = DaMempoolService<
    MempoolNetworkAdapter<Certificate, <Certificate as certificate::Certificate>::Id>,
    MockPool<
        HeaderId,
        VidCertificate,
        <VidCertificate as certificate::vid::VidCertificate>::CertificateId,
    >,
    MempoolVerificationProvider,
>;

pub(crate) const MB16: usize = 1024 * 1024 * 16;
