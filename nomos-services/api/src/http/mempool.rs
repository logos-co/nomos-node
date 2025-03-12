use core::{fmt::Debug, hash::Hash};

use nomos_core::{da::blob::info::DispersedBlobInfo, header::HeaderId};
use nomos_da_sampling::{
    backend::DaSamplingServiceBackend, network::NetworkAdapter as DaSamplingNetworkAdapter,
};
use nomos_da_verifier::backend::VerifierBackend;
use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, GenericDaMempoolService, MempoolMsg,
    TxMempoolService,
};
use nomos_network::backends::NetworkBackend;
use rand::{RngCore, SeedableRng};
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot;

use super::DynError;

pub async fn add_tx<N, A, Item, Key>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    item: Item,
    converter: impl Fn(&Item) -> Key,
) -> Result<(), DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Payload = Item, Key = Key> + Send + Sync + 'static,
    A::Settings: Send + Sync,
    Item: Clone + Debug + Send + Sync + Serialize + for<'de> Deserialize<'de> + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + Send + Serialize + for<'de> Deserialize<'de> + 'static,
{
    let relay = handle
        .relay::<TxMempoolService<A, MockPool<HeaderId, Item, Key>>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MempoolMsg::Add {
            key: converter(&item),
            payload: item,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    receiver.await?.map_err(DynError::from)
}

pub async fn add_blob_info<
    N,
    A,
    Item,
    Key,
    SamplingBackend,
    SamplingAdapter,
    SamplingRng,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
    ApiAdapter,
>(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
    item: A::Payload,
    converter: impl Fn(&A::Payload) -> Key,
) -> Result<(), DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Key = Key> + Send + Sync + 'static,
    A::Payload: DispersedBlobInfo + Into<Item> + Debug,
    A::Settings: Send + Sync,
    Item: Clone + Debug + Send + Sync + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + Send + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = Key> + Send,
    SamplingBackend::BlobId: Debug,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::Settings: Clone,
    SamplingAdapter: DaSamplingNetworkAdapter + Send,
    SamplingRng: SeedableRng + RngCore + Send,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierBackend: VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierNetwork::Settings: Clone,
    ApiAdapter: nomos_da_sampling::api::ApiAdapter + Send + Sync,
{
    let relay = handle
        .relay::<GenericDaMempoolService<
            A,
            MockPool<HeaderId, Item, Key>,
            SamplingBackend,
            SamplingAdapter,
            SamplingRng,
            SamplingStorage,
            DaVerifierBackend,
            DaVerifierNetwork,
            DaVerifierStorage,
            ApiAdapter,
        >>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MempoolMsg::Add {
            key: converter(&item),
            payload: item,
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    receiver.await?.map_err(DynError::from)
}
