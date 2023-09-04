use bytes::Bytes;
use tokio::sync::mpsc::Sender;
use tokio::sync::oneshot;
// internal
use futures::future::join_all;
use nomos_core::wire;
use nomos_http::http::HttpResponse;
use nomos_mempool::network::adapters::waku::{
    WAKU_CARNOT_PUB_SUB_TOPIC, WAKU_CARNOT_TX_CONTENT_TOPIC,
};
use nomos_network::backends::waku::{Waku, WakuBackendMessage};
use nomos_network::NetworkMsg;
use nomos_node::Tx;
use overwatch_rs::services::relay::OutboundRelay;
use waku_bindings::{Multiaddr, WakuMessage};

pub(super) async fn handle_waku_info_req(
    channel: &OutboundRelay<NetworkMsg<Waku>>,
    res_tx: Sender<HttpResponse>,
) -> Result<(), overwatch_rs::DynError> {
    let (sender, receiver) = oneshot::channel();

    channel
        .send(NetworkMsg::Process(WakuBackendMessage::Info {
            reply_channel: sender,
        }))
        .await
        .map_err(|(e, _)| e)?;
    let info = receiver.await.unwrap();
    res_tx.send(Ok(serde_json::to_vec(&info)?.into())).await?;

    Ok(())
}

pub(super) async fn handle_add_conn_req(
    waku_channel: &OutboundRelay<NetworkMsg<Waku>>,
    res_tx: Sender<HttpResponse>,
    payload: Option<Bytes>,
) -> Result<(), overwatch_rs::DynError> {
    if let Some(payload) = payload {
        if let Ok(addrs) = serde_json::from_slice::<Vec<Multiaddr>>(&payload) {
            let reqs: Vec<_> = addrs
                .into_iter()
                .map(|addr| {
                    waku_channel.send(NetworkMsg::Process(WakuBackendMessage::ConnectPeer {
                        addr,
                    }))
                })
                .collect();

            join_all(reqs).await;
        }
        Ok(res_tx.send(Ok(b"".to_vec().into())).await?)
    } else {
        Err(
            format!("Invalid payload, {payload:?}. Empty or couldn't transform into a utf8 String")
                .into(),
        )
    }
}

pub(super) async fn waku_send_transaction(
    network_relay: OutboundRelay<NetworkMsg<Waku>>,
    tx: Tx,
) -> Result<(), overwatch_rs::DynError> {
    let payload = wire::serialize(&tx).expect("Tx serialization failed");
    network_relay
        .send(NetworkMsg::Process(WakuBackendMessage::Broadcast {
            message: WakuMessage::new(
                payload,
                WAKU_CARNOT_TX_CONTENT_TOPIC.clone(),
                1,
                chrono::Utc::now().timestamp_nanos() as usize,
                [],
                false,
            ),
            topic: Some(WAKU_CARNOT_PUB_SUB_TOPIC.clone()),
        }))
        .await
        .map_err(|(e, _)| e)?;

    Ok(())
}
