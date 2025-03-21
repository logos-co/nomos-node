use std::fmt::{Debug, Display};

use nomos_network::{
    backends::libp2p::{Command, Libp2p, Libp2pInfo},
    NetworkMsg, NetworkService,
};
use overwatch::services::AsServiceId;
use tokio::sync::oneshot;

use crate::wait_with_timeout;

pub async fn libp2p_info<RuntimeServiceId>(
    handle: &overwatch::overwatch::handle::OverwatchHandle<RuntimeServiceId>,
) -> Result<Libp2pInfo, overwatch::DynError>
where
    RuntimeServiceId:
        AsServiceId<NetworkService<Libp2p, RuntimeServiceId>> + Debug + Sync + Display,
{
    let relay = handle
        .relay::<NetworkService<Libp2p, RuntimeServiceId>>()
        .await?;
    let (sender, receiver) = oneshot::channel();

    relay
        .send(NetworkMsg::Process(Command::Info { reply: sender }))
        .await
        .map_err(|(e, _)| e)?;

    wait_with_timeout(
        receiver,
        "Timeout while waiting for cl_mempool_metrics".to_owned(),
    )
    .await
}
