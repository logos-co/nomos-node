use bytes::Bytes;
use core::ops::Range;
use nomos_core::da::attestation::Attestation;
use nomos_core::da::blob::Blob;
use nomos_core::da::certificate::metadata::Metadata;
use nomos_core::da::certificate::{self, select::FillSize as FillSizeWithBlobsCertificate};
use nomos_core::da::DaVerifier as CoreDaVerifier;
use nomos_core::header::HeaderId;
use nomos_core::tx::{select::FillSize as FillSizeWithTx, Transaction};
use nomos_da_indexer::storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter;
use nomos_da_indexer::DaMsg;
use nomos_da_indexer::{
    consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter, DataIndexerService,
};
use nomos_da_verifier::backend::VerifierBackend;
use nomos_da_verifier::network::adapters::libp2p::Libp2pAdapter;
use nomos_da_verifier::storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter;
use nomos_da_verifier::{DaVerifierMsg, DaVerifierService};
use nomos_mempool::backend::mockpool::MockPool;
use nomos_mempool::da::verify::kzgrs::DaVerificationProvider as MempoolVerificationProvider;
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter;
use nomos_storage::backends::rocksdb::RocksBackend;
use nomos_storage::backends::StorageSerde;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use overwatch_rs::DynError;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::error::Error;
use std::fmt::Debug;
use std::hash::Hash;
use tokio::sync::oneshot;

pub type DaIndexer<Tx, SS, const SIZE: usize> = DataIndexerService<
    // Indexer specific.
    Bytes,
    IndexerStorageAdapter<SS, kzgrs_backend::dispersal::VidCertificate>,
    CryptarchiaConsensusAdapter<Tx, kzgrs_backend::dispersal::VidCertificate>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<
        Tx,
        kzgrs_backend::dispersal::VidCertificate,
    >,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<
        HeaderId,
        kzgrs_backend::dispersal::VidCertificate,
        <kzgrs_backend::dispersal::VidCertificate as certificate::vid::VidCertificate>::CertificateId,
    >,
    MempoolNetworkAdapter<
        kzgrs_backend::dispersal::Certificate,
        <kzgrs_backend::dispersal::Certificate as certificate::Certificate>::Id,
    >,
    MempoolVerificationProvider,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobsCertificate<SIZE, kzgrs_backend::dispersal::VidCertificate>,
    RocksBackend<SS>,
>;

pub type DaVerifier<A, B, VB, SS> =
    DaVerifierService<VB, Libp2pAdapter<B, A>, VerifierStorageAdapter<A, B, SS>>;

pub async fn add_blob<A, B, VB, SS>(
    handle: &OverwatchHandle,
    blob: B,
) -> Result<Option<A>, DynError>
where
    A: Attestation + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    B: Blob + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <B as Blob>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    VB: VerifierBackend + CoreDaVerifier<DaBlob = B, Attestation = A>,
    <VB as VerifierBackend>::Settings: Clone,
    <VB as CoreDaVerifier>::Error: Error,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle.relay::<DaVerifier<A, B, VB, SS>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(DaVerifierMsg::AddBlob {
            blob,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn get_range<Tx, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
    app_id: <kzgrs_backend::dispersal::VidCertificate as Metadata>::AppId,
    range: Range<<kzgrs_backend::dispersal::VidCertificate as Metadata>::Index>,
) -> Result<
    Vec<(
        <kzgrs_backend::dispersal::VidCertificate as Metadata>::Index,
        Option<Bytes>,
    )>,
    DynError,
>
where
    Tx: Transaction
        + Eq
        + Clone
        + Debug
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle.relay::<DaIndexer<Tx, SS, SIZE>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(DaMsg::GetRange {
            app_id,
            range,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
