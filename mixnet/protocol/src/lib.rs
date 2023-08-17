use sphinx_packet::SphinxPacket;
use std::borrow::Cow;
use std::error::Error;

use tokio::io::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

pub enum Body<'a> {
    SphinxPacket(Box<SphinxPacket>),
    FinalPayload(Cow<'a, [u8]>),
}

impl<'a> Body<'a> {
    pub fn new_sphinx(packet: Box<SphinxPacket>) -> Self {
        Self::SphinxPacket(packet)
    }

    pub fn new_final_owned(data: Vec<u8>) -> Self {
        Self::FinalPayload(Cow::Owned(data))
    }

    pub fn new_final_borrowed(data: &'a [u8]) -> Self {
        Self::FinalPayload(Cow::Borrowed(data))
    }

    pub fn variant_as_u8(&self) -> u8 {
        match self {
            Self::SphinxPacket(_) => 0,
            Self::FinalPayload(_) => 1,
        }
    }

    pub fn data(&self) -> &[u8] {
        match self {
            Self::SphinxPacket(data) => data.payload.as_bytes(),
            Self::FinalPayload(data) => data,
        }
    }

    pub async fn read_body<R>(reader: &mut R) -> Result<Body<'a>, Box<dyn Error>>
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

    pub async fn read_sphinx_packet<R>(reader: &mut R) -> Result<Body<'a>, Box<dyn Error>>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        let packet = SphinxPacket::from_bytes(&buf)?;
        Ok(Self::new_sphinx(Box::new(packet)))
    }

    pub async fn read_final_payload<R>(reader: &mut R) -> Result<Body<'a>, Box<dyn Error>>
    where
        R: AsyncRead + Unpin,
    {
        let mut buf = Vec::new();
        reader.read_to_end(&mut buf).await?;
        Ok(Self::new_final_owned(buf))
    }

    pub async fn write_body<W>(&self, writer: &mut W) -> Result<(), Box<dyn Error>>
    where
        W: AsyncWrite + Unpin + ?Sized,
    {
        writer.write_u8(self.variant_as_u8()).await?;
        writer.write_all(self.data()).await?;
        Ok(())
    }
}
