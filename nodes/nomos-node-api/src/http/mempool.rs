use core::{fmt::Debug, hash::Hash};

use full_replication::{Blob, Certificate};
use nomos_core::{da::blob, tx::Transaction};
use nomos_mempool::{
    backend::mockpool::MockPool, network::NetworkAdapter, Certificate as CertDiscriminant,
    MempoolMsg, MempoolService, Transaction as TxDiscriminant,
};
use nomos_network::backends::NetworkBackend;
use tokio::sync::oneshot;

pub(crate) async fn add_cert<N, A>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    cert: Certificate,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = Certificate, Key = <Blob as blob::Blob>::Hash>
        + Send
        + Sync
        + 'static,
    A::Settings: Send + Sync,
{
    let relay = handle.relay::<MempoolService<A, MockPool<Certificate, <Blob as blob::Blob>::Hash>, CertDiscriminant>>().connect().await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MempoolMsg::Add {
            key: <Certificate as nomos_core::da::certificate::Certificate>::hash(&cert),
            item: cert,
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

pub(crate) async fn add_tx<N, A, T>(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
    tx: T,
) -> Result<(), super::DynError>
where
    N: NetworkBackend,
    A: NetworkAdapter<Backend = N, Item = T, Key = <T as Transaction>::Hash>
        + Send
        + Sync
        + 'static,
    A::Settings: Send + Sync,
    T: Transaction
        + Clone
        + Debug
        + Hash
        + serde::Serialize
        + serde::de::DeserializeOwned
        + Send
        + Sync
        + 'static,
    <T as Transaction>::Hash: std::cmp::Ord + Debug + Send + Sync + 'static,
{
    let relay = handle
        .relay::<MempoolService<A, MockPool<T, <T as Transaction>::Hash>, TxDiscriminant>>()
        .connect()
        .await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MempoolMsg::Add {
            key: Transaction::hash(&tx),
            item: tx,
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
