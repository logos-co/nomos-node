use sphinx_packet::{payload::Payload, SphinxPacket};
use std::{error::Error, io::ErrorKind, net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
};

#[non_exhaustive]
pub enum Body {
    SphinxPacket(Box<SphinxPacket>),
    FinalPayload(Payload),
}

impl Body {
    pub fn new_sphinx(packet: Box<SphinxPacket>) -> Self {
        Self::SphinxPacket(packet)
    }

    pub fn new_final_payload(payload: Payload) -> Self {
        Self::FinalPayload(payload)
    }

    fn variant_as_u8(&self) -> u8 {
        match self {
            Self::SphinxPacket(_) => 0,
            Self::FinalPayload(_) => 1,
        }
    }

    pub async fn read<R>(reader: &mut R) -> Result<Body, Box<dyn Error + Send + Sync + 'static>>
    where
        R: AsyncRead + Unpin,
    {
        let id = reader.read_u8().await?;
        match id {
            0 => Self::read_sphinx_packet(reader).await,
            1 => Self::read_final_payload(reader).await,
            _ => Err("Invalid body type".into()),
        }
    }

    fn sphinx_packet_from_bytes(
        data: &[u8],
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        let packet = SphinxPacket::from_bytes(data)?;
        Ok(Self::new_sphinx(Box::new(packet)))
    }

    async fn read_sphinx_packet<R>(
        reader: &mut R,
    ) -> Result<Body, Box<dyn Error + Send + Sync + 'static>>
    where
        R: AsyncRead + Unpin,
    {
        let size = reader.read_u64().await?;
        let mut buf = vec![0; size as usize];
        reader.read_exact(&mut buf).await?;
        Self::sphinx_packet_from_bytes(&buf)
    }

    pub fn final_payload_from_bytes(
        data: &[u8],
    ) -> Result<Self, Box<dyn Error + Send + Sync + 'static>> {
        let payload = Payload::from_bytes(data)?;
        Ok(Self::new_final_payload(payload))
    }

    async fn read_final_payload<R>(
        reader: &mut R,
    ) -> Result<Body, Box<dyn Error + Send + Sync + 'static>>
    where
        R: AsyncRead + Unpin,
    {
        let size = reader.read_u64().await?;
        let mut buf = vec![0; size as usize];
        reader.read_exact(&mut buf).await?;

        Self::final_payload_from_bytes(&buf)
    }

    pub async fn write<W>(
        &self,
        writer: &mut W,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let variant = self.variant_as_u8();
        writer.write_u8(variant).await?;
        match self {
            Self::SphinxPacket(packet) => {
                let data = packet.to_bytes();
                writer.write_u64(data.len() as u64).await?;
                writer.write_all(&data).await?;
            }
            Self::FinalPayload(payload) => {
                let data = payload.as_bytes();
                writer.write_u64(data.len() as u64).await?;
                writer.write_all(data).await?;
            }
        }
        Ok(())
    }
}

pub async fn retry_backoff(
    peer_addr: SocketAddr,
    max_retries: usize,
    retry_delay: Duration,
    body: Body,
    socket: Arc<Mutex<TcpStream>>,
) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
    for idx in 0..max_retries {
        // backoff
        let wait = retry_delay * (idx as u32);

        tokio::time::sleep(wait).await;
        let mut socket = socket.lock().await;
        match body.write(&mut *socket).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                if let Some(err) = e.downcast_ref::<std::io::Error>() {
                    match err.kind() {
                        ErrorKind::Unsupported
                        | ErrorKind::NotFound
                        | ErrorKind::PermissionDenied
                        | ErrorKind::Other => {}
                        _ => {
                            // update the connection
                            if let Ok(tcp) = TcpStream::connect(peer_addr).await {
                                *socket = tcp;
                            }
                        }
                    }
                }
            }
        }
    }
    Err(format!("Failure after {max_retries} retries").into())
}
