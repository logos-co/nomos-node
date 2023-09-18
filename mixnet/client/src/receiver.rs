use std::net::SocketAddr;

use futures::{stream, Stream, StreamExt};
use mixnet_protocol::Body;
use nym_sphinx::{
    chunking::{fragment::Fragment, reconstruction::MessageReconstructor},
    message::{NymMessage, PaddedMessage},
    Payload,
};
use tokio::net::TcpStream;

use super::error::*;
use crate::MixnetClientError;

// Receiver accepts TCP connections to receive incoming payloads from the Mixnet.
pub struct Receiver {
    node_address: SocketAddr,
}

impl Receiver {
    pub fn new(node_address: SocketAddr) -> Self {
        Self { node_address }
    }

    pub async fn run(&self) -> Result<impl Stream<Item = Result<Vec<u8>>> + Send + 'static> {
        let Ok(socket) = TcpStream::connect(self.node_address).await else {
            return Err(MixnetClientError::MixnetNodeConnectError);
        };

        Ok(Self::message_stream(Box::pin(Self::fragment_stream(
            socket,
        ))))
    }

    fn fragment_stream(socket: TcpStream) -> impl Stream<Item = Result<Fragment>> + Send + 'static {
        stream::unfold(socket, move |mut socket| {
            async move {
                let Ok(body) = Body::read(&mut socket).await else {
                    // TODO: Maybe this is a hard error and the stream is corrupted? In that case stop the stream
                    return Some((Err(MixnetClientError::MixnetNodeStreamClosed), socket));
                };

                match body {
                    Body::SphinxPacket(_) => {
                        Some((Err(MixnetClientError::UnexpectedStreamBody), socket))
                    }
                    Body::FinalPayload(payload) => {
                        Some((Self::fragment_from_payload(payload), socket))
                    }
                    _ => unreachable!(),
                }
            }
        })
    }

    fn message_stream(
        fragment_stream: impl Stream<Item = Result<Fragment>> + Send + Unpin + 'static,
    ) -> impl Stream<Item = Result<Vec<u8>>> + Send + 'static {
        // MessageReconstructor buffers all received fragments
        // and eventually returns reconstructed messages.
        let message_reconstructor: MessageReconstructor = Default::default();

        stream::unfold(
            (fragment_stream, message_reconstructor),
            |(mut fragment_stream, mut message_reconstructor)| async move {
                let result =
                    Self::reconstruct_message(&mut fragment_stream, &mut message_reconstructor)
                        .await;
                Some((result, (fragment_stream, message_reconstructor)))
            },
        )
    }

    fn fragment_from_payload(payload: Payload) -> Result<Fragment> {
        let Ok(payload_plaintext) = payload.recover_plaintext() else {
            return Err(MixnetClientError::InvalidPayload);
        };
        let Ok(fragment) = Fragment::try_from_bytes(&payload_plaintext) else {
            return Err(MixnetClientError::InvalidPayload);
        };
        Ok(fragment)
    }

    async fn reconstruct_message(
        fragment_stream: &mut (impl Stream<Item = Result<Fragment>> + Send + Unpin + 'static),
        message_reconstructor: &mut MessageReconstructor,
    ) -> Result<Vec<u8>> {
        // Read fragments until at least one message is fully reconstructed.
        while let Some(next) = fragment_stream.next().await {
            match next {
                Ok(fragment) => {
                    if let Some(message) =
                        Self::try_reconstruct_message(fragment, message_reconstructor)
                    {
                        return Ok(message);
                    }
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }

        // fragment_stream closed before messages are fully reconstructed
        Err(MixnetClientError::MixnetNodeStreamClosed)
    }

    fn try_reconstruct_message(
        fragment: Fragment,
        message_reconstructor: &mut MessageReconstructor,
    ) -> Option<Vec<u8>> {
        let reconstruction_result = message_reconstructor.insert_new_fragment(fragment);
        match reconstruction_result {
            Some((padded_message, _)) => {
                let message = Self::remove_padding(padded_message).unwrap();
                Some(message)
            }
            None => None,
        }
    }

    fn remove_padding(msg: Vec<u8>) -> Result<Vec<u8>> {
        let padded_message = PaddedMessage::new_reconstructed(msg);
        // we need this because PaddedMessage.remove_padding requires it for other NymMessage types.
        let dummy_num_mix_hops = 0;

        match padded_message.remove_padding(dummy_num_mix_hops)? {
            NymMessage::Plain(msg) => Ok(msg),
            _ => todo!("return error"),
        }
    }
}
