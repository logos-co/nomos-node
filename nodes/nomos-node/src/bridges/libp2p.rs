// std
// crates
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
// internal
use nomos_http::http::HttpResponse;
use nomos_network::backends::libp2p::{Command, Libp2p};
use nomos_network::NetworkMsg;
use overwatch_rs::services::relay::OutboundRelay;

#[cfg(feature = "libp2p")]
pub(super) async fn handle_libp2p_info_req(
    channel: &OutboundRelay<NetworkMsg<Libp2p>>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let (sender, receiver) = oneshot::channel();

    channel
        .send(NetworkMsg::Process(Command::Info { reply: sender }))
        .await
        .map_err(|(e, _)| e)?;

    let info = receiver.await.unwrap();
    res_tx.send(Ok(serde_json::to_vec(&info)?.into())).await?;

    Ok(())
}
