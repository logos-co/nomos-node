use core::{fmt::Debug, hash::Hash};

use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, Discriminant, MempoolMsg, MempoolService,
};
use nomos_network::backends::NetworkBackend;
use tokio::sync::oneshot;

pub async fn add<N, A, D, Item, Key>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    item: Item,
    converter: impl Fn(&Item) -> Key,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = Item, Key = Key> + Send + Sync + 'static,
    A::Settings: Send + Sync,
    D: Discriminant + Send,
    Item: Clone + Debug + Send + Sync + 'static + Hash,
    Key: Clone + Debug + Ord + Hash + 'static,
{
    let relay = handle
        .relay::<MempoolService<A, MockPool<Item, Key>, D>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MempoolMsg::Add {
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
