use std::{error::Error, net::SocketAddr};

use futures::{stream, Stream, TryStreamExt};
use mixnet_protocol::Body;
use nym_sphinx::{
    chunking::{fragment::Fragment, reconstruction::MessageReconstructor},
    message::{NymMessage, PaddedMessage},
    Payload,
};
use tokio::net::TcpStream;

// Receiver accepts TCP connections to receive incoming payloads from the Mixnet.
pub struct Receiver;

impl Receiver {
    pub async fn run(
        node_address: SocketAddr,
    ) -> Result<impl Stream<Item = Result<Vec<u8>, Box<dyn Error>>> + Send + 'static, Box<dyn Error>>
    {
        let socket = TcpStream::connect(node_address).await?;

        // MessageReconstructor buffers all received fragments (payloads)
        // and eventually returns reconstructed messages.
        let message_reconstructor: MessageReconstructor = Default::default();

        Ok(stream::unfold(
            (socket, message_reconstructor),
            |(mut socket, mut message_reconstructor)| async move {
                loop {
                    let Ok(body) = Body::read(&mut socket).await else {
                        // TODO: Maybe this is a hard error and the stream is corrupted? In that case stop the stream
                        return Some((Err("Could not read body from socket".into()), (socket, message_reconstructor)));
                    };
                    match body {
                        Body::SphinxPacket(_) => {
                            return Some((Err("received sphinx packet not expected".into()), (socket, message_reconstructor)));
                        }
                        Body::FinalPayload(payload) => {
                            match Self::handle_payload(payload, &mut message_reconstructor).await {
                                Ok(Some(reconstructed_message)) => {
                                    return Some((Ok(reconstructed_message), (socket, message_reconstructor)));
                                },
                                Ok(None) => {
                                    // A payload has been received but the message isn't yet reconstructed completely.
                                    // Read more payloads.
                                    continue;
                                }
                                Err(e) => {
                                    return Some((Err(format!("Could not handle payload: {e}").into()), (socket, message_reconstructor)));
                                },
                            }
                        }
                    }
                }
            },
        )
        .into_stream())
    }

    async fn handle_payload(
        payload: Payload,
        message_reconstructor: &mut MessageReconstructor,
    ) -> Result<Option<Vec<u8>>, Box<dyn Error>> {
        let fragment = Fragment::try_from_bytes(&payload.recover_plaintext()?)?;

        let reconstruction_result = message_reconstructor.insert_new_fragment(fragment);
        match reconstruction_result {
            Some((padded_message, _)) => {
                tracing::debug!("a reconstructed message reconstructed");
                Ok(Some(Self::remove_padding(padded_message)?))
            }
            None => Ok(None),
        }
    }

    fn remove_padding(msg: Vec<u8>) -> Result<Vec<u8>, Box<dyn Error>> {
        let padded_message = PaddedMessage::new_reconstructed(msg);
        // we need this because PaddedMessage.remove_padding requires it for other NymMessage types.
        let dummy_num_mix_hops = 0;

        match padded_message.remove_padding(dummy_num_mix_hops)? {
            NymMessage::Plain(msg) => Ok(msg),
            _ => todo!("return error"),
        }
    }
}
