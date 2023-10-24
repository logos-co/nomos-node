use std::{fmt::Debug, hash::Hash};

use consensus_engine::overlay::{RandomBeaconState, RoundRobin, TreeOverlay};
use full_replication::Certificate;
use nomos_consensus::{
    network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter, CarnotConsensus,
    CarnotInfo, ConsensusMsg,
};
use nomos_core::{
    da::{
        blob,
        certificate::{self, select::FillSize as FillSizeWithBlobsCertificate},
    },
    tx::{select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter,
};
use nomos_network::{
    backends::libp2p::{Command, Libp2p, Libp2pInfo},
    NetworkMsg, NetworkService,
};
use nomos_storage::backends::{sled::SledBackend, StorageSerde};
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

pub type Carnot<Tx, SS, const SIZE: usize> = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Certificate, <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash>,
    MempoolLibp2pAdapter<
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    TreeOverlay<RoundRobin, RandomBeaconState>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobsCertificate<SIZE, Certificate>,
    SledBackend<SS>,
>;

pub(crate) async fn carnot_info<Tx, SS, const SIZE: usize>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<CarnotInfo, overwatch_rs::DynError>
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

pub(crate) async fn libp2p_info(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<Libp2pInfo, overwatch_rs::DynError> {
    let relay = handle.relay::<NetworkService<Libp2p>>().connect().await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(NetworkMsg::Process(Command::Info { reply: sender }))
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
