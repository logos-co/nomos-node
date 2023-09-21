// std
// crates
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
// internal
use nomos_core::wire;
use nomos_http::http::HttpResponse;
use nomos_network::backends::libp2p::{Command, Libp2p};
use nomos_network::NetworkMsg;
use nomos_node::Tx;
use overwatch_rs::services::relay::OutboundRelay;

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

pub(super) async fn libp2p_send_transaction(
    network_relay: OutboundRelay<NetworkMsg<Libp2p>>,
    tx: Tx,
) -> Result<(), overwatch_rs::DynError> {
    let payload = wire::serialize(&tx).expect("Tx serialization failed");
    network_relay
        .send(NetworkMsg::Process(Command::Broadcast {
            topic: nomos_mempool::network::adapters::libp2p::CARNOT_TX_TOPIC.to_string(),
            message: payload.into_boxed_slice(),
        }))
        .await
        .map_err(|(e, _)| e)?;

    Ok(())
}
