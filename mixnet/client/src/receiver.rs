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
        Ok(stream::unfold(socket, |mut socket| async move {
            let Ok(body) = Body::read(&mut socket).await else {
                    // TODO: Maybe this is a hard error and the stream is corrupted? In that case stop the stream
                    return Some((Err("Could not read body from socket".into()), socket));
                };
            let mut message_reconstructor: MessageReconstructor = Default::default();
            match body {
                Body::SphinxPacket(_) => {
                    Some((Err("received sphinx packet not expected".into()), socket))
                }
                Body::FinalPayload(payload) => Some((
                    Self::handle_payload(payload, &mut message_reconstructor).await,
                    socket,
                )),
            }
        })
        .into_stream())
    }

    async fn handle_payload(
        payload: Payload,
        message_reconstructor: &mut MessageReconstructor,
    ) -> Result<Vec<u8>, Box<dyn Error>> {
        let fragment = Fragment::try_from_bytes(&payload.recover_plaintext()?)?;

        let reconstruction_result = message_reconstructor.insert_new_fragment(fragment);
        let message = if let Some((padded_message, _)) = reconstruction_result {
            tracing::debug!("sending a reconstructed message to the local");
            Self::remove_padding(padded_message)?
        } else {
            // TODO: polish error message
            return Err("received a fragment that did not complete a message".into());
        };

        Ok(message)
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
