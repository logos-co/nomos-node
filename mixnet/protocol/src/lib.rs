use sphinx_packet::SphinxPacket;
use std::error::Error;
use std::io::Cursor;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub enum Body<'a> {
    SphinxPacket(Box<SphinxPacket>),
    FinalPayload(Box<dyn AsyncRead + Unpin + Send + 'a>),
}

impl<'a> Body<'a> {
    pub fn new_sphinx(packet: Box<SphinxPacket>) -> Self {
        Self::SphinxPacket(packet)
    }

    pub fn new_final_owned(data: Vec<u8>) -> Self {
        Self::FinalPayload(Box::new(Cursor::new(data)))
    }

    pub fn new_final_stream(data: impl AsyncRead + Unpin + Send + 'a) -> Self {
        Self::FinalPayload(Box::new(data))
    }

    fn variant_as_u8(&self) -> u8 {
        match self {
            Self::SphinxPacket(_) => 0,
            Self::FinalPayload(_) => 1,
        }
    }

    pub async fn read<R>(reader: &'a mut R) -> Result<Body<'a>, Box<dyn Error>>
    where
        R: AsyncRead + Unpin + Send,
    {
        let id = reader.read_u8().await?;
        match id {
            0 => Self::read_sphinx_packet(reader).await,
            1 => Self::read_final_payload(reader).await,
            _ => Err("Invalid body type".into()),
        }
    }

    async fn read_sphinx_packet<R>(reader: &mut R) -> Result<Body<'a>, Box<dyn Error>>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        let packet = SphinxPacket::from_bytes(&buf)?;
        Ok(Self::new_sphinx(Box::new(packet)))
    }

    async fn read_final_payload<R>(reader: &'a mut R) -> Result<Body<'a>, Box<dyn Error>>
    where
        R: AsyncRead + Unpin + Send,
    {
        Ok(Self::new_final_stream(reader))
    }

    pub async fn write<W>(self, writer: &mut W) -> Result<(), Box<dyn Error>>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        let variant = self.variant_as_u8();
        writer.write_u8(variant).await?;
        match self {
            Body::SphinxPacket(packet) => {
                writer.write_all(packet.payload.as_bytes()).await?;
            }
            Body::FinalPayload(mut reader) => {
                tokio::io::copy(&mut reader, writer).await?;
            }
        }
        Ok(())
    }
}
