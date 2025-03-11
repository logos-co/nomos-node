use std::{fmt::Debug, net::IpAddr};

use async_trait::async_trait;
use common_http_client::CommonHttpClient;
use kzgrs_backend::common::blob::{DaBlob, DaBlobSharedCommitments};
use libp2p_identity::PeerId;
use multiaddr::Multiaddr;
use nomos_core::da::BlobId;
use nomos_da_network_core::SubnetworkId;
use overwatch::DynError;
use rand::prelude::IteratorRandom;
use serde::{Deserialize, Serialize};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;
use tracing::error;
use url::Url;

use crate::api::ApiAdapter;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ApiAdapterSettings<Membership> {
    pub membership: Membership,
    pub api_port: u16,
}

pub struct HttApiAdapter<Membership> {
    pub clients: Vec<CommonHttpClient>,
    pub membership: Membership,
}

#[async_trait]
impl<Membership> ApiAdapter for HttApiAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type Settings = ApiAdapterSettings<Membership>;
    type Blob = DaBlob;
    type BlobId = BlobId;
    type Commitments = DaBlobSharedCommitments;
    fn new(settings: Self::Settings) -> Self {
        // This part will be removed on a later PR when `base_url` is not part of
        // `client` anymore
        let clients = settings
            .membership
            .members()
            .iter()
            .filter_map(|peer_id| {
                if let Some(address) = settings.membership.get_address(peer_id) {
                    let ip = get_ip_from_multiaddr(&address).unwrap();
                    Some(CommonHttpClient::new(
                        Url::parse(&format!("http://{}:{}", ip, settings.api_port)).unwrap(),
                        None,
                    ))
                } else {
                    None
                }
            })
            .collect();
        Self {
            clients,
            membership: settings.membership,
        }
    }

    async fn request_commitments(
        &self,
        blob_id: Self::BlobId,
        reply_channel: oneshot::Sender<Option<Self::Commitments>>,
    ) -> Result<(), DynError> {
        let Some(client) = self.clients.iter().choose(&mut rand::thread_rng()) else {
            error!("No clients available");
            if reply_channel.send(None).is_err() {
                error!("Failed to send commitments reply");
            }
            return Ok(());
        };
        match client.get_commitments::<Self::Blob>(blob_id).await {
            Ok(commitments) => {
                if reply_channel.send(commitments).is_err() {
                    error!("Failed to send commitments reply");
                }
            }
            Err(e) => {
                error!("Failed to get commitments: {}", e);
            }
        }

        Ok(())
    }
}

fn get_ip_from_multiaddr(multiaddr: &Multiaddr) -> Option<IpAddr> {
    multiaddr.iter().find_map(|protocol| match protocol {
        multiaddr::Protocol::Ip4(ipv4) => Some(IpAddr::V4(ipv4)),
        multiaddr::Protocol::Ip6(ipv6) => Some(IpAddr::V6(ipv6)),
        _ => None,
    })
}
