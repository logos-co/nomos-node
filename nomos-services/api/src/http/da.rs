use bytes::Bytes;
use core::ops::Range;
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_core::da::blob::{metadata::Metadata, select::FillSize as FillSizeWithBlobs, Blob};
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

pub type DaIndexer<Tx, C, V, SS, const SIZE: usize> = DataIndexerService<
    // Indexer specific.
    Bytes,
    IndexerStorageAdapter<SS, V>,
    CryptarchiaConsensusAdapter<Tx, V>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, V>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, V, [u8; 32]>,
    MempoolNetworkAdapter<C, <C as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobs<SIZE, V>,
    RocksBackend<SS>,
>;

pub type DaVerifier<A, B, VB, SS> =
    DaVerifierService<VB, Libp2pAdapter<B, A>, VerifierStorageAdapter<A, B, SS>>;

pub async fn add_blob<A, B, VB, SS>(
    handle: &OverwatchHandle,
    blob: B,
) -> Result<Option<()>, DynError>
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    B: Blob + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <B as Blob>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    VB: VerifierBackend + CoreDaVerifier<DaBlob = B>,
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

pub async fn get_range<Tx, C, V, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
    app_id: <V as Metadata>::AppId,
    range: Range<<V as Metadata>::Index>,
) -> Result<Vec<(<V as Metadata>::Index, Option<Bytes>)>, DynError>
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
    C: DispersedBlobInfo<BlobId = [u8; 32]>
        + Clone
        + Debug
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <C as DispersedBlobInfo>::BlobId: Clone + Send + Sync,
    V: DispersedBlobInfo<BlobId = [u8; 32]>
        + From<C>
        + Eq
        + Debug
        + Metadata
        + Hash
        + Clone
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <V as DispersedBlobInfo>::BlobId: Debug + Clone + Ord + Hash,
    <V as Metadata>::AppId: AsRef<[u8]> + Serialize + Clone + Send + Sync,
    <V as Metadata>::Index:
        AsRef<[u8]> + Serialize + DeserializeOwned + Clone + PartialOrd + Send + Sync,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle
        .relay::<DaIndexer<Tx, C, V, SS, SIZE>>()
        .connect()
        .await?;
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
