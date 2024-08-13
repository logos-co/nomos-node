// use crate::protocol::SAMPLING_PROTOCOL;
// use crate::sampling::behaviour::SampleError;
// use futures::future::BoxFuture;
// use libp2p::core::UpgradeInfo;
// use libp2p::swarm::SubstreamProtocol;
// use libp2p::{InboundUpgrade, StreamProtocol};
// use nomos_da_messages::sampling::SampleReq;
// use std::iter;
//
// #[derive(Debug, Clone, Default)]
// pub struct SamplingProtocol {}
//
// impl UpgradeInfo for SamplingProtocol {
//     type Info = StreamProtocol;
//     type InfoIter = iter::Once<Self::Info>;
//
//     fn protocol_info(&self) -> Self::InfoIter {
//         iter::once(SAMPLING_PROTOCOL)
//     }
// }
//
// impl<TransportSocket> InboundUpgrade<TransportSocket> for SamplingProtocol {
//     type Output = SampleReq;
//     type Error = SampleError;
//     type Future = BoxFuture<'static, Result<Self::Output, Self::Error>>;
//
//     fn upgrade_inbound(self, socket: TransportSocket, info: Self::Info) -> Self::Future {
//         todo!()
//     }
// }
