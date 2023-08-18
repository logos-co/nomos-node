use std::{
    error::Error,
    net::SocketAddr,
    sync::{Arc, Mutex},
};

use nym_sphinx::{
    chunking::{fragment::Fragment, reconstruction::MessageReconstructor},
    message::{NymMessage, PaddedMessage},
    Payload,
};
use tokio::{
    io::AsyncReadExt,
    net::{TcpListener, TcpStream},
    sync::broadcast,
};

// Receiver accepts TCP connections to receive incoming payloads from the Mixnet.
pub struct Receiver;

impl Receiver {
    pub async fn run(
        listen_addr: SocketAddr,
        message_tx: broadcast::Sender<Vec<u8>>,
    ) -> Result<(), Box<dyn Error>> {
        let listener = TcpListener::bind(listen_addr).await?;
        let message_reconstructor: Arc<Mutex<MessageReconstructor>> = Default::default();

        loop {
            match listener.accept().await {
                Ok((socket, remote_addr)) => {
                    tracing::debug!("Accepted incoming connection from {remote_addr:?}");

                    let message_tx = message_tx.clone();
                    let message_reconstructor = message_reconstructor.clone();

                    tokio::spawn(async {
                        if let Err(e) =
                            Self::handle_connection(socket, message_tx, message_reconstructor).await
                        {
                            tracing::error!("failed to handle conn: {e}");
                        }
                    });
                }
                Err(e) => tracing::warn!("Failed to accept incoming connection: {e}"),
            }
        }
    }

    async fn handle_connection(
        mut socket: TcpStream,
        message_tx: broadcast::Sender<Vec<u8>>,
        message_reconstructor: Arc<Mutex<MessageReconstructor>>,
    ) -> Result<(), Box<dyn Error>> {
        let mut buf = Vec::new();
        socket.read_to_end(&mut buf).await?;

        let payload = Payload::from_bytes(&buf)?.recover_plaintext()?;
        let fragment = Fragment::try_from_bytes(&payload)?;

        if let Some((padded_message, _)) = {
            let mut reconstructor = message_reconstructor.lock().unwrap();
            reconstructor.insert_new_fragment(fragment)
        } {
            tracing::debug!("sending a reconstructed message to the local");
            let message = Self::remove_padding(padded_message)?;
            message_tx.send(message)?;
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
