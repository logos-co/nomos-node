use std::{collections::HashSet, fmt::Debug, pin::Pin, time::Duration};

use futures::{stream::BoxStream, Stream, StreamExt};
use kzgrs_backend::common::blob::DaBlob;
use nomos_core::da::BlobId;
use nomos_da_network_core::{
    protocols::{
        dispersal::executor::behaviour::DispersalExecutorEvent, sampling::behaviour::SamplingError,
    },
    PeerId, SubnetworkId,
};
use nomos_da_network_service::{
    backends::libp2p::{
        common::SamplingEvent,
        executor::{
            DaNetworkEvent, DaNetworkEventKind, DaNetworkExecutorBackend, ExecutorDaNetworkMessage,
        },
    },
    DaNetworkMsg, NetworkService,
};
use overwatch_rs::{
    services::{relay::OutboundRelay, ServiceData},
    DynError,
};
use subnetworks_assignations::MembershipHandler;
use tokio::sync::oneshot;

use crate::adapters::network::DispersalNetworkAdapter;

pub struct Libp2pNetworkAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    outbound_relay: OutboundRelay<DaNetworkMsg<DaNetworkExecutorBackend<Membership>>>,
}

impl<Membership> Libp2pNetworkAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    async fn start_sampling(
        &self,
        blob_id: BlobId,
        subnets: &[SubnetworkId],
    ) -> Result<(), DynError> {
        for id in subnets {
            let subnetwork_id = id;
            self.outbound_relay
                .send(DaNetworkMsg::Process(
                    ExecutorDaNetworkMessage::RequestSample {
                        blob_id,
                        subnetwork_id: *subnetwork_id,
                    },
                ))
                .await
                .expect("RequestSample message should have been sent");
        }
        Ok(())
    }
}

#[async_trait::async_trait]
impl<Membership> DispersalNetworkAdapter for Libp2pNetworkAdapter<Membership>
where
    Membership: MembershipHandler<NetworkId = SubnetworkId, Id = PeerId>
        + Clone
        + Debug
        + Send
        + Sync
        + 'static,
{
    type NetworkService = NetworkService<DaNetworkExecutorBackend<Membership>>;

    type SubnetworkId = Membership::NetworkId;

    fn new(outbound_relay: OutboundRelay<<Self::NetworkService as ServiceData>::Message>) -> Self {
        Self { outbound_relay }
    }

    async fn disperse(
        &self,
        subnetwork_id: Self::SubnetworkId,
        da_blob: DaBlob,
    ) -> Result<(), DynError> {
        self.outbound_relay
            .send(DaNetworkMsg::Process(
                ExecutorDaNetworkMessage::RequestDispersal {
                    subnetwork_id,
                    da_blob: Box::new(da_blob),
                },
            ))
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)
    }

    async fn dispersal_events_stream(
        &self,
    ) -> Result<
        Pin<Box<dyn Stream<Item = Result<(BlobId, Self::SubnetworkId), DynError>> + Send>>,
        DynError,
    > {
        let (sender, receiver) = oneshot::channel();
        self.outbound_relay
            .send(DaNetworkMsg::Subscribe {
                kind: DaNetworkEventKind::Dispersal,
                sender,
            })
            .await
            .map_err(|(e, _)| Box::new(e) as DynError)?;
        receiver
            .await
            .map_err(|e| Box::new(e) as DynError)
            .map(|stream| {
                Box::pin(stream.filter_map(|event| async {
                    match event {
                        DaNetworkEvent::Sampling(_) | DaNetworkEvent::Verifying(_) => None,
                        DaNetworkEvent::Dispersal(DispersalExecutorEvent::DispersalError {
                            error,
                        }) => Some(Err(Box::new(error) as DynError)),
                        DaNetworkEvent::Dispersal(DispersalExecutorEvent::DispersalSuccess {
                            blob_id,
                            subnetwork_id,
                        }) => Some(Ok((blob_id, subnetwork_id))),
                    }
                }))
                    as BoxStream<'static, Result<(BlobId, Self::SubnetworkId), DynError>>
            })
    }

    async fn get_blob_samples(
        &self,
        blob_id: BlobId,
        subnets: &[SubnetworkId],
        cooldown: Duration,
    ) -> Result<(), DynError> {
        let expected_count = subnets.len();
        let mut success_count = 0;

        let mut pending_subnets: HashSet<SubnetworkId> = subnets.iter().copied().collect();

        let (stream_sender, stream_receiver) = oneshot::channel();

        self.outbound_relay
            .send(DaNetworkMsg::Subscribe {
                kind: DaNetworkEventKind::Sampling,
                sender: stream_sender,
            })
            .await
            .map_err(|(error, _)| error)?;

        self.start_sampling(blob_id, subnets).await?;

        let stream = match stream_receiver.await {
            Ok(stream) => stream,
            Err(error) => return Err(Box::new(error) as DynError),
        };

        enum SampleOutcome {
            Success(u16),
            Retry(u16),
        }

        let mut stream = tokio_stream::StreamExt::filter_map(stream, move |event| match event {
            DaNetworkEvent::Sampling(SamplingEvent::SamplingSuccess {
                blob_id: event_blob_id,
                ref blob,
            }) if event_blob_id == blob_id => Some(SampleOutcome::Success(blob.column_idx)),
            DaNetworkEvent::Sampling(SamplingEvent::SamplingError { ref error }) => match error {
                SamplingError::Protocol {
                    error,
                    subnetwork_id,
                    ..
                } if error.blob_id == blob_id => Some(SampleOutcome::Retry(*subnetwork_id)),
                SamplingError::Deserialize {
                    blob_id: event_blob_id,
                    subnetwork_id,
                    ..
                } if *event_blob_id == blob_id => Some(SampleOutcome::Retry(*subnetwork_id)),
                SamplingError::BlobNotFound {
                    blob_id: event_blob_id,
                    subnetwork_id,
                    ..
                } if *event_blob_id == blob_id => Some(SampleOutcome::Retry(*subnetwork_id)),
                _ => None,
            },
            _ => None,
        });

        loop {
            tokio::select! {
                Some(event) = stream.next() => {
                    match event {
                        SampleOutcome::Success(subnetwork_id) => {
                            success_count += 1;
                            pending_subnets.remove(&subnetwork_id);
                            if success_count >= expected_count {
                                return Ok(());
                            }
                        }
                        SampleOutcome::Retry(subnetwork_id) => {
                            pending_subnets.insert(subnetwork_id);
                        }
                    }
                }
                () = tokio::time::sleep(cooldown) => {
                    if !pending_subnets.is_empty() {
                        let retry_subnets: Vec<_> = pending_subnets.iter().copied().collect();
                        self.start_sampling(blob_id, &retry_subnets).await?;
                    }
                }
            }
        }
    }
}
