use core::{fmt::Debug, hash::Hash};
use nomos_core::{da::certificate::Certificate, header::HeaderId};
use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, verify::MempoolVerificationProvider,
    DaMempoolService, MempoolMsg, TxMempoolService,
};
use nomos_network::backends::NetworkBackend;
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

pub async fn add_cert<N, A, V, Item, Key>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    item: A::Payload,
    converter: impl Fn(&A::Payload) -> Key,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Key = Key> + Send + Sync + 'static,
    A::Payload: Certificate + Into<Item> + Debug,
    A::Settings: Send + Sync,
    V: MempoolVerificationProvider<
        Payload = A::Payload,
        Parameters = <A::Payload as Certificate>::VerificationParameters,
    >,
    Item: Clone + Debug + Send + Sync + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + 'static,
{
    let relay = handle
        .relay::<DaMempoolService<A, MockPool<HeaderId, Item, Key>, V>>()
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
