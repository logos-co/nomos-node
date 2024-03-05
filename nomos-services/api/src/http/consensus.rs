use std::{fmt::Debug, hash::Hash};

use nomos_da::auth::mock::MockDaAuth;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use serde::{de::DeserializeOwned, Serialize};
use tokio::sync::oneshot;

use carnot_consensus::{
    network::adapters::libp2p::Libp2pAdapter as ConsensusLibp2pAdapter, CarnotConsensus,
    CarnotInfo, ConsensusMsg,
};
use carnot_engine::{
    overlay::{RandomBeaconState, RoundRobin, TreeOverlay},
    Block, BlockId,
};
use full_replication::Certificate;
use nomos_core::{
    da::{
        blob,
        certificate::{
            self, mock::MockKeyStore, select::FillSize as FillSizeWithBlobsCertificate,
            verify::DaCertificateVerifier,
        },
    },
    tx::{mock::MockTxVerifier, select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolLibp2pAdapter,
};
use nomos_storage::backends::{sled::SledBackend, StorageSerde};

pub type Carnot<Tx, SS, const SIZE: usize> = CarnotConsensus<
    ConsensusLibp2pAdapter,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    MempoolLibp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockTxVerifier,
    MockPool<Certificate, <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash>,
    MempoolLibp2pAdapter<
        Certificate,
        <<Certificate as certificate::Certificate>::Blob as blob::Blob>::Hash,
    >,
    DaCertificateVerifier<[u8; 32], MockKeyStore<MockDaAuth>, Certificate>,
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
