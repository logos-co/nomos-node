use nomos_mempool::openapi::MempoolMetrics;

#[utoipa::path(
    get,
    path = "/da/metrics",
    responses(
        (status = 200, description = "Get the mempool metrics of the da service", body = MempoolMetrics),
        (status = 500, description = "Internal server error", body = String),
    )
)]
pub async fn da_mempool_metrics(handle: overwatch_rs::overwatch::handle::OverwatchHandle) -> Result<MempoolMetrics, String> {
    todo!()
}


async fn handle_mempool_metrics_req<K, V>(
  mempool_channel: &OutboundRelay<MempoolMsg<K, V>>,
  res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
  let (sender, receiver) = oneshot::channel();
  mempool_channel
      .send(MempoolMsg::Metrics {
          reply_channel: sender,
      })
      .await
      .map_err(|(e, _)| e)?;

  let metrics: MempoolMetrics = receiver.await.unwrap();
  res_tx
      // TODO: use serde to serialize metrics
      .send(Ok(format!(
          "{{\"pending_items\": {}, \"last_item\": {}}}",
          metrics.pending_items, metrics.last_item_timestamp
      )
      .into()))
      .await?;

  Ok(())
}

#[test]
fn test() {
}