use std::{error::Error, marker::Unpin, net::SocketAddr};

use futures::{Sink, SinkExt};
use mixnet_protocol::Body;
use nym_sphinx::{
    chunking::{fragment::Fragment, reconstruction::MessageReconstructor},
    message::{NymMessage, PaddedMessage},
    Payload,
};
use tokio::net::{TcpSocket, TcpStream};

// Receiver accepts TCP connections to receive incoming payloads from the Mixnet.
pub struct Receiver;

impl Receiver {
    pub async fn run(
        node_address: SocketAddr,
        message_tx: impl Sink<Vec<u8>> + Unpin + Clone,
    ) -> Result<(), Box<dyn Error>> {
        let mut socket = TcpStream::connect(node_address).await?;
        let mut message_reconstructor: MessageReconstructor = Default::default();

        loop {
            let body = Body::read(&mut socket).await?;
            match body {
                Body::SphinxPacket(_) => return Err("received sphinx packet not expected".into()),
                Body::FinalPayload(payload) => {
                    Self::handle_payload(payload, message_tx.clone(), &mut message_reconstructor)
                        .await?
                }
            }
        }
    }

    async fn handle_payload(
        payload: Payload,
        mut message_tx: impl Sink<Vec<u8>> + Unpin,
        message_reconstructor: &mut MessageReconstructor,
    ) -> Result<(), Box<dyn Error>> {
        let fragment = Fragment::try_from_bytes(&payload.recover_plaintext()?)?;

        let reconstruction_result = message_reconstructor.insert_new_fragment(fragment);
        if let Some((padded_message, _)) = reconstruction_result {
            tracing::debug!("sending a reconstructed message to the local");
            let message = Self::remove_padding(padded_message)?;
            if (message_tx.send(message).await).is_err() {
                return Err("failed to send message to the sink".into());
            }
        }

        Ok(())
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
