use sphinx_packet::{payload::Payload, SphinxPacket};

use std::{io::ErrorKind, net::SocketAddr, sync::Arc, time::Duration};

use tokio::{
    io::{self, AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt},
    net::TcpStream,
    sync::Mutex,
};

pub type Result<T> = core::result::Result<T, ProtocolError>;

#[derive(Debug, thiserror::Error)]
pub enum ProtocolError {
    #[error("Unknown body type {0}")]
    UnknownBodyType(u8),
    #[error("{0}")]
    InvalidSphinxPacket(sphinx_packet::Error),
    #[error("{0}")]
    InvalidPayload(sphinx_packet::Error),
    #[error("{0}")]
    IO(#[from] io::Error),
    #[error("fail to send packet, reach maximum retries {0}")]
    ReachMaxRetries(usize),
}

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

    pub async fn read<R>(reader: &mut R) -> Result<Body>
    where
        R: AsyncRead + Unpin,
    {
        let id = reader.read_u8().await?;
        match id {
            0 => Self::read_sphinx_packet(reader).await,
            1 => Self::read_final_payload(reader).await,
            id => Err(ProtocolError::UnknownBodyType(id)),
        }
    }

    fn sphinx_packet_from_bytes(data: &[u8]) -> Result<Self> {
        SphinxPacket::from_bytes(data)
            .map(|packet| Self::new_sphinx(Box::new(packet)))
            .map_err(ProtocolError::InvalidPayload)
    }

    async fn read_sphinx_packet<R>(reader: &mut R) -> Result<Body>
    where
        R: AsyncRead + Unpin,
    {
        let size = reader.read_u64().await?;
        let mut buf = vec![0; size as usize];
        reader.read_exact(&mut buf).await?;
        Self::sphinx_packet_from_bytes(&buf)
    }

    pub fn final_payload_from_bytes(data: &[u8]) -> Result<Self> {
        Payload::from_bytes(data)
            .map(Self::new_final_payload)
            .map_err(ProtocolError::InvalidPayload)
    }

    async fn read_final_payload<R>(reader: &mut R) -> Result<Body>
    where
        R: AsyncRead + Unpin,
    {
        let size = reader.read_u64().await?;
        let mut buf = vec![0; size as usize];
        reader.read_exact(&mut buf).await?;

        Self::final_payload_from_bytes(&buf)
    }

    pub async fn write<W>(&self, writer: &mut W) -> Result<()>
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
) -> Result<()> {
    for idx in 0..max_retries {
        // backoff
        let wait = Duration::from_millis((retry_delay.as_millis() as u64).pow(idx as u32));
        tokio::time::sleep(wait).await;
        let mut socket = socket.lock().await;
        match body.write(&mut *socket).await {
            Ok(_) => return Ok(()),
            Err(e) => {
                match &e {
                    ProtocolError::IO(err) => {
                        match err.kind() {
                            ErrorKind::Unsupported => return Err(e),
                            _ => {
                                // update the connection
                                if let Ok(tcp) = TcpStream::connect(peer_addr).await {
                                    *socket = tcp;
                                }
                            }
                        }
                    }
                    _ => return Err(e),
                }
            }
        }
    }
    Err(ProtocolError::ReachMaxRetries(max_retries))
}
