use std::{fmt::Debug, hash::Hash};

use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

use carnot_consensus::{
    network::adapters::p2p::P2pAdapter as ConsensusNetworkAdapter, CarnotConsensus, CarnotInfo,
    ConsensusMsg,
};
use carnot_engine::{
    overlay::{RandomBeaconState, RoundRobin, TreeOverlay},
    Block, BlockId,
};
use full_replication::Certificate;
use nomos_core::{
    da::{
        blob,
        certificate::{self, select::FillSize as FillSizeWithBlobsCertificate},
    },
    tx::{select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::p2p::P2pAdapter as MempoolNetworkAdapter,
};
use nomos_storage::backends::{sled::SledBackend, StorageSerde};

pub type Carnot<Tx, SS, const SIZE: usize> = CarnotConsensus<
    ConsensusNetworkAdapter,
    MockPool<BlockId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<
        BlockId,
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    MempoolNetworkAdapter<
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobsCertificate<SIZE, Certificate>,
    SledBackend<SS>,
>;

pub async fn carnot_info<Tx, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
) -> Result<CarnotInfo, super::DynError>
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle.relay::<Carnot<Tx, SS, SIZE>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn carnot_blocks<Tx, SS, const SIZE: usize>(
    handle: &OverwatchHandle,
    from: Option<BlockId>,
    to: Option<BlockId>,
) -> Result<Vec<Block>, super::DynError>
where
    Tx: Transaction + Clone + Debug + Hash + Serialize + DeserializeOwned + Send + Sync + 'static,
    <Tx as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle.relay::<Carnot<Tx, SS, SIZE>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::GetBlocks {
            from,
            to,
            tx: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
