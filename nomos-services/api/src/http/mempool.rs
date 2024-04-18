use core::{fmt::Debug, hash::Hash};
use nomos_core::header::HeaderId;
use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, TxMempoolMsg, TxMempoolService,
};
use nomos_network::backends::NetworkBackend;
use tokio::sync::oneshot;

pub async fn add<N, A, Item, Key>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    item: Item,
    converter: impl Fn(&Item) -> Key,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = Item, Key = Key> + Send + Sync + 'static,
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
        .send(TxMempoolMsg::Add {
            key: converter(&item),
            item,
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
