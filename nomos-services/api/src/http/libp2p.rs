use nomos_network::{
    backends::libp2p::{Command, Libp2p, Libp2pInfo},
    NetworkMsg, NetworkService,
};
use tokio::sync::oneshot;

use crate::{wait_with_timeout, HTTP_REQUEST_TIMEOUT};

pub async fn libp2p_info(
    handle: &overwatch::overwatch::handle::OverwatchHandle,
) -> Result<Libp2pInfo, overwatch::DynError> {
    let relay = handle.relay::<NetworkService<Libp2p>>().connect().await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(NetworkMsg::Process(Command::Info { reply: sender }))
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        HTTP_REQUEST_TIMEOUT,
        "Timeout while waiting for cl_mempool_metrics".to_owned(),
    )
    .await
}
