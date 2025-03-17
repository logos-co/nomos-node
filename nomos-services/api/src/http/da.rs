use core::ops::Range;
use std::{error::Error, fmt::Debug, hash::Hash};

use kzgrs_backend::common::share::DaShare;
use nomos_blend_service::network::libp2p::Libp2pAdapter as BlendNetworkAdapter;
use nomos_core::{
    da::{
        blob::{info::DispersedBlobInfo, metadata, select::FillSize as FillSizeWithBlobs, Share},
        BlobId, DaVerifier as CoreDaVerifier,
    },
    header::HeaderId,
    tx::{select::FillSize as FillSizeWithTx, Transaction},
};
use nomos_da_dispersal::{
    adapters::{mempool::DaMempoolAdapter, network::DispersalNetworkAdapter},
    backend::DispersalBackend,
    DaDispersalMsg, DispersalService,
};
use nomos_da_indexer::{
    consensus::adapters::cryptarchia::CryptarchiaConsensusAdapter,
    storage::adapters::rocksdb::RocksAdapter as IndexerStorageAdapter, DaMsg, DataIndexerService,
};
use nomos_da_network_core::SubnetworkId;
use nomos_da_network_service::{
    backends::{
        libp2p::{
            common::MonitorCommand, executor::ExecutorDaNetworkMessage, validator::DaNetworkMessage,
        },
        NetworkBackend,
    },
    DaNetworkMsg, NetworkService,
};
use nomos_da_sampling::backend::DaSamplingServiceBackend;
use nomos_da_verifier::{
    backend::VerifierBackend, network::adapters::validator::Libp2pAdapter,
    storage::adapters::rocksdb::RocksAdapter as VerifierStorageAdapter, DaVerifierMsg,
    DaVerifierService,
};
use nomos_libp2p::PeerId;
use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::libp2p::Libp2pAdapter as MempoolNetworkAdapter,
};
use nomos_storage::backends::{rocksdb::RocksBackend, StorageSerde};
use overwatch::{overwatch::handle::OverwatchHandle, DynError};
use rand::{RngCore, SeedableRng};
use serde::{de::DeserializeOwned, Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;

use crate::{wait_with_timeout, HTTP_REQUEST_TIMEOUT};

pub type DaIndexer<
    Tx,
    C,
    V,
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
> = DataIndexerService<
    // Indexer specific.
    DaShare,
    IndexerStorageAdapter<SS, V>,
    CryptarchiaConsensusAdapter<Tx, V>,
    // Cryptarchia specific, should be the same as in `Cryptarchia` type above.
    cryptarchia_consensus::network::adapters::libp2p::LibP2pAdapter<Tx, V>,
    cryptarchia_consensus::blend::adapters::libp2p::LibP2pAdapter<BlendNetworkAdapter, Tx, V>,
    MockPool<HeaderId, Tx, <Tx as Transaction>::Hash>,
    MempoolNetworkAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<HeaderId, V, [u8; 32]>,
    MempoolNetworkAdapter<C, <C as DispersedBlobInfo>::BlobId>,
    FillSizeWithTx<SIZE, Tx>,
    FillSizeWithBlobs<SIZE, V>,
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

pub type DaVerifier<Blob, Membership, VerifierBackend, StorageSerializer> = DaVerifierService<
    VerifierBackend,
    Libp2pAdapter<Membership>,
    VerifierStorageAdapter<Blob, StorageSerializer>,
>;

pub type DaDispersal<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata> =
    DispersalService<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>;

pub type DaNetwork<Backend> = NetworkService<Backend>;

pub async fn add_share<A, S, M, VB, SS>(
    handle: &OverwatchHandle,
    share: S,
) -> Result<Option<()>, DynError>
where
    A: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    S: Share + Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <S as Share>::BlobId: AsRef<[u8]> + Send + Sync + 'static,
    <S as Share>::ShareIndex: AsRef<[u8]> + Send + Sync + 'static,
    <S as Share>::LightShare: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    <S as Share>::SharesCommitments: Serialize + DeserializeOwned + Clone + Send + Sync + 'static,
    M: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    VB: VerifierBackend + CoreDaVerifier<DaShare = S>,
    <VB as VerifierBackend>::Settings: Clone,
    <VB as CoreDaVerifier>::Error: Error,
    SS: StorageSerde + Send + Sync + 'static,
{
    let relay = handle.relay::<DaVerifier<S, M, VB, SS>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(DaVerifierMsg::AddShare {
            share,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for add share".to_owned(),
    )
    .await
}

pub async fn get_range<
    Tx,
    C,
    V,
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
    handle: &OverwatchHandle,
    app_id: <V as metadata::Metadata>::AppId,
    range: Range<<V as metadata::Metadata>::Index>,
) -> Result<Vec<(<V as metadata::Metadata>::Index, Vec<DaShare>)>, DynError>
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
        + metadata::Metadata
        + Hash
        + Clone
        + Serialize
        + DeserializeOwned
        + Send
        + Sync
        + 'static,
    <V as DispersedBlobInfo>::BlobId: Debug + Clone + Ord + Hash,
    <V as metadata::Metadata>::AppId: AsRef<[u8]> + Serialize + Clone + Send + Sync,
    <V as metadata::Metadata>::Index:
        AsRef<[u8]> + Serialize + DeserializeOwned + Clone + PartialOrd + Send + Sync,
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
        .relay::<DaIndexer<
            Tx,
            C,
            V,
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
        .send(DaMsg::GetRange {
            app_id,
            range,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for get range".to_owned(),
    )
    .await
}

pub async fn disperse_data<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>(
    handle: &OverwatchHandle,
    data: Vec<u8>,
    metadata: Metadata,
) -> Result<(), DynError>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
    Backend: DispersalBackend<
            NetworkAdapter = NetworkAdapter,
            MempoolAdapter = MempoolAdapter,
            Metadata = Metadata,
        > + Send
        + Sync,
    Backend::Settings: Clone + Send + Sync,
    NetworkAdapter: DispersalNetworkAdapter<SubnetworkId = Membership::NetworkId> + Send,
    MempoolAdapter: DaMempoolAdapter,
    Metadata: metadata::Metadata + Debug + Send + 'static,
{
    let relay = handle
        .relay::<DaDispersal<Backend, NetworkAdapter, MempoolAdapter, Membership, Metadata>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();
    relay
        .send(DaDispersalMsg::Disperse {
            data,
            metadata,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for disperse data".to_owned(),
    )
    .await?
}

pub async fn block_peer<B>(handle: &OverwatchHandle, peer_id: PeerId) -> Result<bool, DynError>
where
    B: NetworkBackend + 'static + Send,
    B::Message: PeerMessagesFactory,
{
    let relay = handle.relay::<NetworkService<B>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    let message = B::Message::create_block_message(peer_id, sender);
    relay
        .send(DaNetworkMsg::Process(message))
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for block peer".to_string(),
    )
    .await
}

pub async fn unblock_peer<B>(handle: &OverwatchHandle, peer_id: PeerId) -> Result<bool, DynError>
where
    B: NetworkBackend + 'static + Send,
    B::Message: PeerMessagesFactory,
{
    let relay = handle.relay::<NetworkService<B>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    let message = B::Message::create_unblock_message(peer_id, sender);
    relay
        .send(DaNetworkMsg::Process(message))
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for unblock peer".to_owned(),
    )
    .await
}

pub async fn blacklisted_peers<B>(handle: &OverwatchHandle) -> Result<Vec<PeerId>, DynError>
where
    B: NetworkBackend + 'static + Send,
    B::Message: PeerMessagesFactory,
{
    let relay = handle.relay::<NetworkService<B>>().connect().await?;
    let (sender, receiver) = oneshot::channel();
    let message = B::Message::create_blacklisted_message(sender);
    relay
        .send(DaNetworkMsg::Process(message))
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for blacklisted peers".to_owned(),
    )
    .await
}

// Factory for generating messages for peers (validator and executor).
pub trait PeerMessagesFactory {
    fn create_block_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self;
    fn create_unblock_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self;
    fn create_blacklisted_message(sender: oneshot::Sender<Vec<PeerId>>) -> Self;
}

impl PeerMessagesFactory for DaNetworkMessage {
    fn create_block_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self {
        Self::PeerRequest(MonitorCommand::BlockPeer(peer_id, sender))
    }

    fn create_unblock_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self {
        Self::PeerRequest(MonitorCommand::UnblockPeer(peer_id, sender))
    }

    fn create_blacklisted_message(sender: oneshot::Sender<Vec<PeerId>>) -> Self {
        Self::PeerRequest(MonitorCommand::BlacklistedPeers(sender))
    }
}

impl PeerMessagesFactory for ExecutorDaNetworkMessage {
    fn create_block_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self {
        Self::PeerRequest(MonitorCommand::BlockPeer(peer_id, sender))
    }

    fn create_unblock_message(peer_id: PeerId, sender: oneshot::Sender<bool>) -> Self {
        Self::PeerRequest(MonitorCommand::UnblockPeer(peer_id, sender))
    }

    fn create_blacklisted_message(sender: oneshot::Sender<Vec<PeerId>>) -> Self {
        Self::PeerRequest(MonitorCommand::BlacklistedPeers(sender))
    }
}
