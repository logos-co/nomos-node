use nomos_network::{
    backends::libp2p::{Command, Libp2p, Libp2pInfo},
    NetworkMsg, NetworkService,
};
use tokio::sync::oneshot;

pub(crate) async fn libp2p_info(
    handle: &overwatch_rs::overwatch::handle::OverwatchHandle,
) -> Result<Libp2pInfo, overwatch_rs::DynError> {
    let relay = handle.relay::<NetworkService<Libp2p>>().connect().await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(NetworkMsg::Process(Command::Info { reply: sender }))
        .await
        .map_err(|(e, _)| e)?;

    Ok(receiver.await?)
}
