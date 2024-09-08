use core::{fmt::Debug, hash::Hash};
use nomos_core::{da::blob::info::DispersedBlobInfo, header::HeaderId};
use nomos_da_sampling::{
    backend::DaSamplingServiceBackend, network::NetworkAdapter as DaSamplingNetworkAdapter,
};
use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, DaMempoolService, MempoolMsg,
    TxMempoolService,
};
use nomos_network::backends::NetworkBackend;
use rand::{RngCore, SeedableRng};
use tokio::sync::oneshot;

pub async fn add_tx<N, A, Item, Key>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    item: Item,
    converter: impl Fn(&Item) -> Key,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Payload = Item, Key = Key> + Send + Sync + 'static,
    A::Settings: Send + Sync,
    Item: Clone + Debug + Send + Sync + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + 'static,
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

    match receiver.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(())) => Err("mempool error".into()),
        Err(e) => Err(e.into()),
    }
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
>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    item: A::Payload,
    converter: impl Fn(&A::Payload) -> Key,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Key = Key> + Send + Sync + 'static,
    A::Payload: DispersedBlobInfo + Into<Item> + Debug,
    A::Settings: Send + Sync,
    Item: Clone + Debug + Send + Sync + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + 'static,
    SamplingBackend: DaSamplingServiceBackend<SamplingRng, BlobId = Key> + Send,
    SamplingBackend::BlobId: Debug,
    SamplingBackend::Blob: Debug + 'static,
    SamplingBackend::Settings: Clone,
    SamplingAdapter: DaSamplingNetworkAdapter + Send,
    SamplingRng: SeedableRng + RngCore + Send,
    SamplingStorage: nomos_da_sampling::storage::DaStorageAdapter,
{
    let relay = handle
        .relay::<DaMempoolService<
            A,
            MockPool<HeaderId, Item, Key>,
            SamplingBackend,
            SamplingAdapter,
            SamplingRng,
            SamplingStorage,
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

    match receiver.await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(())) => Err("mempool error".into()),
        Err(e) => Err(e.into()),
    }
}
