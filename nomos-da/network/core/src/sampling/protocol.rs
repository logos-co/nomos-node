use crate::protocol::SAMPLING_PROTOCOL;
use crate::sampling::behaviour::SampleError;

use futures::channel::oneshot;
use futures::io::{AsyncRead, AsyncWrite, AsyncWriteExt};
use libp2p::core::UpgradeInfo;
use libp2p::{InboundUpgrade, OutboundUpgrade, StreamProtocol};
use nomos_da_messages::sampling::{SampleReq, SampleRes};
use nomos_da_messages::{pack_message, unpack_from_reader};
use std::future::Future;
use std::iter;
use std::pin::Pin;

#[derive(Debug, Clone)]
pub struct SamplingProtocolRequest(SampleReq);

impl UpgradeInfo for SamplingProtocolRequest {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(SAMPLING_PROTOCOL)
    }
}

impl<TransportSocket> InboundUpgrade<TransportSocket> for SamplingProtocolRequest
where
    TransportSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = SamplingProtocolResponse;
    type Error = SampleError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_inbound(self, mut socket: TransportSocket, _info: Self::Info) -> Self::Future {
        Box::pin(async move {
            let request: SampleReq = unpack_from_reader(&mut socket).await?;
            let (response_sender, response_receiver) = oneshot::channel();
            Ok(SamplingProtocolResponse {
                request,
                response_receiver,
                response_sender,
            })
        })
    }
}

#[derive(Debug)]
pub struct SamplingProtocolResponse {
    request: SampleReq,
    response_receiver: oneshot::Receiver<SampleRes>,
    response_sender: oneshot::Sender<SampleRes>,
}

impl UpgradeInfo for SamplingProtocolResponse {
    type Info = StreamProtocol;
    type InfoIter = iter::Once<Self::Info>;

    fn protocol_info(&self) -> Self::InfoIter {
        iter::once(SAMPLING_PROTOCOL)
    }
}

impl<TransportSocket> OutboundUpgrade<TransportSocket> for SamplingProtocolResponse
where
    TransportSocket: AsyncRead + AsyncWrite + Send + Unpin + 'static,
{
    type Output = ();
    type Error = SampleError;
    type Future = Pin<Box<dyn Future<Output = Result<Self::Output, Self::Error>> + Send>>;

    fn upgrade_outbound(self, mut socket: TransportSocket, _info: Self::Info) -> Self::Future {
        Box::pin(async move {
            let response = self.response_receiver.await?;
            let bytes = pack_message(&response)?;
            socket.write_all(&bytes).await?;
            socket.flush().await?;
            Ok(())
        })
    }
}
