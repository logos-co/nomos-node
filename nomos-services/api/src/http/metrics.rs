use nomos_metrics::{Metrics, MetricsMsg};
use tokio::sync::oneshot;

pub async fn gather(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<String, super::DynError> {
    let relay = handle.relay::<Metrics>().connect().await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(MetricsMsg::Gather {
            reply_channel: sender,
        })
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
