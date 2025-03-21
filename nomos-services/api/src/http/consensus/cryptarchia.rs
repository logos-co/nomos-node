use std::{fmt::Debug, hash::Hash};

use cryptarchia_consensus::{
    blend::adapters::libp2p::LibP2pAdapter as BlendAdapter,
    network::adapters::libp2p::LibP2pAdapter as ConsensusNetworkAdapter, ConsensusMsg,
    CryptarchiaConsensus, CryptarchiaInfo,
};
use kzgrs_backend::dispersal::BlobInfo;
use nomos_blend_service::network::libp2p::Libp2pAdapter as BlendNetworkAdapter;
use nomos_core::{
    da::{
        blob::{self, select::FillSize as FillSizeWithBlobs},
        BlobId,
    },
    header::HeaderId,
    tx::{select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
};
use nomos_storage::backends::{rocksdb::RocksBackend, StorageSerde};
use overwatch::overwatch::handle::OverwatchHandle;
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use tokio::sync::oneshot;

use crate::http::DynError;

pub type Cryptarchia<
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    const SIZE: usize,
> = CryptarchiaConsensus<
    ConsensusNetworkAdapter<Tx, BlobInfo>,
    BlendAdapter<BlendNetworkAdapter, Tx, BlobInfo>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, BlobInfo, <BlobInfo as blob::info::DispersedBlobInfo>::BlobId>,
    MempoolNetworkAdapter<BlobInfo, <BlobInfo as blob::info::DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobs<SIZE, BlobInfo>,
    RocksBackend<SS>,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
>;

pub async fn cryptarchia_info<
    'a,
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    const SIZE: usize,
>(
    handle: &'a OverwatchHandle,
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
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    let relay = handle
        .relay::<Cryptarchia<
            Tx,
            SS,
            SamplingBackend,
            SamplingNetworkAdapter,
            SamplingRng,
            SamplingStorage,
            DaVerifierBackend,
            DaVerifierNetwork,
            DaVerifierStorage,
            TimeBackend,
            ApiAdapter,
            SIZE,
        >>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(ConsensusMsg::Info { tx: sender })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}

pub async fn cryptarchia_headers<
    'a,
    Tx,
    SS,
    SamplingBackend,
    SamplingNetworkAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    TimeBackend,
    ApiAdapter,
    const SIZE: usize,
>(
    handle: &'a OverwatchHandle,
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
    <Tx as Transaction>::Hash:
        std::cmp::Ord + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static,
    SS: StorageSerde + Send + Sync + 'static,
    SamplingRng: SeedableRng + RngCore,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = BlobId> + Send,
    SamplingBackend::Settings: Clone,
    SamplingBackend::Share: Debug + 'static,
    SamplingBackend::BlobId: Debug + 'static,
    SamplingNetworkAdapter: nomos_da_sampling::network::NetworkAdapter,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
    TimeBackend: nomos_time::backends::TimeBackend,
    TimeBackend::Settings: Clone + Send + Sync,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    let relay = handle
        .relay::<Cryptarchia<
            Tx,
            SS,
            SamplingBackend,
            SamplingNetworkAdapter,
            SamplingRng,
            SamplingStorage,
            DaVerifierBackend,
            DaVerifierNetwork,
            DaVerifierStorage,
            TimeBackend,
            ApiAdapter,
            SIZE,
        >>()
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
