use sphinx_packet::{payload::Payload, SphinxPacket};
use std::{
    error::Error,
    net::{Ipv4Addr, Ipv6Addr, SocketAddr, SocketAddrV4, SocketAddrV6},
};

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

/// A crc32 checksum of the packet.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct PacketId(u32);

impl PacketId {
    pub const fn new(id: u32) -> Self {
        Self(id)
    }

    pub fn from_bytes(bytes: &[u8]) -> Self {
        Self(crc32fast::hash(bytes))
    }
}

impl core::fmt::Display for PacketId {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        write!(f, "{}", self.0)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct AckResponse {
    pub id: PacketId,
    pub sender: SocketAddr,
}

#[non_exhaustive]
pub enum Body {
    SphinxPacket(Box<SphinxPacket>),
    FinalPayload(Payload),
    AckResponse(AckResponse),
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
            Self::AckResponse(_) => 2,
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
            2 => Self::read_ack_response(reader).await,
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

    async fn read_ack_response<R>(
        reader: &mut R,
    ) -> Result<Body, Box<dyn Error + Send + Sync + 'static>>
    where
        R: AsyncRead + Unpin,
    {
        let cks = reader.read_u32().await?;
        let id = PacketId(cks);
        let socket_type = reader.read_u8().await?;
        match socket_type {
            0 => {
                let mut buf = [0; 6];
                reader.read_exact(&mut buf).await?;
                let id = PacketId::from_bytes(&buf);
                let ip_addr = Ipv4Addr::new(buf[0], buf[1], buf[2], buf[3]);
                let port = u16::from_be_bytes([buf[4], buf[5]]);
                Ok(Self::AckResponse(AckResponse {
                    id,
                    sender: SocketAddr::V4(SocketAddrV4::new(ip_addr, port)),
                }))
            }
            1 => {
                let mut buf = [0; 18];
                reader.read_exact(&mut buf).await?;
                // Manually inline here for better performance?
                let ip_addr = Ipv6Addr::new(
                    u16::from_be_bytes([buf[0], buf[1]]),
                    u16::from_be_bytes([buf[2], buf[3]]),
                    u16::from_be_bytes([buf[4], buf[5]]),
                    u16::from_be_bytes([buf[6], buf[7]]),
                    u16::from_be_bytes([buf[8], buf[9]]),
                    u16::from_be_bytes([buf[10], buf[11]]),
                    u16::from_be_bytes([buf[12], buf[13]]),
                    u16::from_be_bytes([buf[14], buf[15]]),
                );
                let port = u16::from_be_bytes([buf[16], buf[17]]);
                Ok(Self::AckResponse(AckResponse {
                    id,
                    sender: SocketAddr::V6(SocketAddrV6::new(ip_addr, port, 0, 0)),
                }))
            }
            _ => Err("Invalid socket type".into()),
        }
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

    pub async fn write_bytes<W>(
        writer: &mut W,
        kind: u8,
        data: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        match kind {
            0 => {
                writer.write_u64(data.len() as u64).await?;
                writer.write_all(data).await?;
            }
            1 => {
                writer.write_u64(data.len() as u64).await?;
                writer.write_all(data).await?;
            }
            _ => unreachable!(),
        }
        Ok(())
    }

    pub async fn write_sphinx_packet_bytes<W>(
        writer: &mut W,
        data: &[u8],
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        writer.write_u64(data.len() as u64).await?;
        writer.write_all(data).await?;
        Ok(())
    }

    /// Returns `Some(data)` if the body is a SphinxPacket for packet send retry.
    pub async fn write<W>(
        self,
        writer: &mut W,
    ) -> Result<Option<Vec<u8>>, Box<dyn Error + Send + Sync + 'static>>
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
                Ok(Some(data))
            }
            Self::FinalPayload(payload) => {
                let data = payload.as_bytes();
                writer.write_u64(data.len() as u64).await?;
                writer.write_all(data).await?;
                Ok(None)
            }
            Self::AckResponse(ack) => {
                writer.write_u32(ack.id.0).await?;
                match ack.sender {
                    SocketAddr::V4(addr) => {
                        writer.write_u8(0).await?;
                        writer.write_all(&addr.ip().octets()).await?;
                        writer.write_u16(addr.port()).await?;
                    }
                    SocketAddr::V6(addr) => {
                        writer.write_u8(1).await?;
                        writer.write_all(&addr.ip().octets()).await?;
                        writer.write_u16(addr.port()).await?;
                    }
                }
                Ok(None)
            }
        }
    }
}
