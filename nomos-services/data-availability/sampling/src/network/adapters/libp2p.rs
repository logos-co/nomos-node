// // std
// use futures::Stream;
// use overwatch_rs::DynError;
// use rand::Rng;
// use std::collections::HashMap;
// use std::fmt::{Debug, Formatter};
// use std::marker::PhantomData;
// use std::Display;
//
// // crates
//
// // internal
// use crate::network::NetworkAdapter;
// use crate::DaSamplingServiceSettings;
// use nomos_core::da::BlobId;
// use nomos_core::wire;
// use nomos_da_network_service::backends::libp2p::validator::{DaNetworkMessage, SamplingEvent};
// use nomos_da_network_service::backends::NetworkBackend;
// use nomos_da_network_service::{DaNetworkMsg, NetworkService};
// use overwatch_rs::services::relay::OutboundRelay;
// use overwatch_rs::services::ServiceData;
// use serde::de::DeserializeOwned;
// use serde::Serialize;
// use tokio_stream::wrappers::BroadcastStream;
// use tokio_stream::StreamExt;
// use tracing::debug;
//
// #[derive(Debug, thiserror::Error)]
// pub enum SamplingError {
//     #[error("BlobId [{0}] not triggered for sampling")]
//     NoSuchPendingBlobId(BlobId),
// }
//
// pub enum SamplingTracking {
//     SamplingCompleted,
//     SamplingInProcess,
// }
//
// impl std::error::Error for SamplingError {}
//
// pub struct DaNetworkSamplingSettings {
//     pub num_samples: u16,
//     pub subnet_size: u16,
// }
//
// pub struct SamplingContext {
//     blob_id: BlobId,
//     subnets: Vec<u16>,
// }
//
// pub struct Libp2pAdapter {
//     settings: DaNetworkSamplingSettings,
//     network_relay: OutboundRelay<<NetworkService<NetworkBackend> as ServiceData>::Message>,
//     pending_sampling: HashMap<BlobId, SamplingContext>,
// }
//
// #[async_trait::async_trait]
// impl<B> NetworkAdapter for Libp2pAdapter {
//     type Backend = NetworkBackend;
//     type Settings = DaNetworkSamplingSettings;
//
//     async fn new(
//         settings: Self::Settings,
//         network_relay: OutboundRelay<<NetworkService<Self::Backend> as ServiceData>::Message>,
//     ) -> Self {
//         let instance = Self {
//             settings: settings,
//             pending_sampling: HashMap::new(),
//             network_relay,
//         };
//         instance.listen_to_sampling_messages();
//         instance
//     }
//
//     async fn start_sampling(&self, blob_id: BlobId) -> Result<(), DynError> {
//         let mut rng = rand::thread_rng();
//         let ctx: SamplingContext = SamplingContext {
//             blob_id: (blob_id),
//             subnets: (),
//         };
//         for i in self.settings.num_samples {
//             let subnetwork_id = rng.gen_range(0..self.settings.subnet_size);
//             ctx.subnets.push(subnetwork_id);
//             self.network_relay
//                 .send(DaNetworkMessage::RequestSample {
//                     blob_id,
//                     subnetwork_id,
//                 })
//                 .await
//                 .expect("RequestSample message should have been sent")?
//         }
//         self.pending_sampling[blob_id] = ctx;
//         Ok(())
//     }
//
//     async fn listen_to_sampling_messages(
//         &self,
//     ) -> Box<dyn Stream<Item = SamplingEvent> + Unpin + Send> {
//         self.network_relay
//             .send(DaNetworkMsg::Subscribe(SamplingEvent))
//             .await
//             .expect("Network backend should be ready");
//     }
//
//     async fn handle_sampling_event(
//         &self,
//         success: bool,
//         blob_id: BlobId,
//         subnet_id: u16,
//     ) -> Result<(SamplingTracking), DynError> {
//         if !self.pending_sampling.contains_key(&blob_id) {
//             return Err(SamplingError::NoSuchPendingBlobId);
//         }
//         match success {
//             True => {
//                 self.pending_sampling[blob_id].push(subnet_id);
//                 if self.pending_sampling[blob_id].len() == self.settings.num_samples {
//                     Ok((SamplingTracking::SamplingCompleted));
//                 }
//                 Ok((SamplingTracking::SamplingInProcess))
//             }
//             False => {}
//         }
//     }
// }
