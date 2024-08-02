use std::{fmt::Debug, hash::Hash};

use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

use crate::http::DynError;
use cryptarchia_consensus::{
    network::adapters::libp2p::LibP2pAdapter as ConsensusNetworkAdapter, ConsensusMsg,
    CryptarchiaConsensus, CryptarchiaInfo,
};
use kzgrs_backend::dispersal::BlobInfo;
use nomos_core::{
    da::blob::{self, select::FillSize as FillSizeWithBlobs},
    header::HeaderId,
    tx::{select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
};
use nomos_storage::backends::{rocksdb::RocksBackend, StorageSerde};

pub type Cryptarchia<Tx, SS, const SIZE: usize> = CryptarchiaConsensus<
    ConsensusNetworkAdapter<Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as blob::info::DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as blob::info::DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobs<SIZE, BlobInfo>,
    RocksBackend<SS>,
>;

pub async fn cryptarchia_info<Tx, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
) -> Result<CryptarchiaInfo, DynError>
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
    let relay = handle
        .relay::<Cryptarchia<Tx, SS, SIZE>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn cryptarchia_headers<Tx, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
    from: Option<HeaderId>,
    to: Option<HeaderId>,
) -> Result<Vec<HeaderId>, DynError>
where
    Tx: Transaction
        + Clone
        + Debug
        + Eq
        + Hash
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle
        .relay::<Cryptarchia<Tx, SS, SIZE>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::GetHeaders {
            from,
            to,
            tx: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
