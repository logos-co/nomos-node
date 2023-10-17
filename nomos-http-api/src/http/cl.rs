use nomos_mempool::{
    backend::mockpool::MockPool,
    network::adapters::libp2p::Libp2pAdapter,
    openapi::{MempoolMetrics, Status},
    Transaction as TxDiscriminant,
    MempoolMsg, MempoolService,
};
use tokio::sync::oneshot;


use bytes::Bytes;
use nomos_core::tx::{Transaction, TransactionHasher};
use serde::{Deserialize, Serialize};
use std::hash::Hash;

#[derive(Clone, Debug, Serialize, Deserialize, Hash, PartialEq, Eq)]
pub struct Tx(pub String);

fn hash_tx(tx: &Tx) -> String {
    tx.0.clone()
}

impl Transaction for Tx {
    const HASHER: TransactionHasher<Self> = hash_tx;
    type Hash = String;

    fn as_bytes(&self) -> Bytes {
        self.0.as_bytes().to_vec().into()
    }
}

type ClMempoolService = MempoolService<
    Libp2pAdapter<Tx, <Tx as Transaction>::Hash>,
    MockPool<Tx, <Tx as Transaction>::Hash>,
    TxDiscriminant,
>;

pub(crate) async fn cl_mempool_metrics(
  handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<MempoolMetrics, super::DynError> {
  let relay = handle.relay::<ClMempoolService>().connect().await.unwrap();
  let (sender, receiver) = oneshot::channel();
  relay
      .send(MempoolMsg::Metrics {
          reply_channel: sender,
      })
      .await
      .map_err(|(e, _)| e)?;

  Ok(receiver.await.unwrap())
}

pub(crate) async fn cl_mempool_status(
  handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
  items: Vec<<Tx as Transaction>::Hash>,
) -> Result<Vec<Status>, super::DynError> {
  let relay = handle.relay::<ClMempoolService>().connect().await.unwrap();
  let (sender, receiver) = oneshot::channel();
  relay
      .send(MempoolMsg::Status {
          items,
          reply_channel: sender,
      })
      .await
      .map_err(|(e, _)| e)?;

  Ok(receiver.await.unwrap())
}