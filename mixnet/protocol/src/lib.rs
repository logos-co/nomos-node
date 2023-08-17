use std::error::Error;

use tokio::io::{AsyncWrite, AsyncWriteExt};

pub enum BodyType {
    SphinxPacket,
    FinalPayload,
}

impl BodyType {
    pub fn as_u8(&self) -> u8 {
        match self {
            Self::SphinxPacket => 0,
            Self::FinalPayload => 1,
        }
    }

    pub fn from_u8(value: u8) -> Self {
        match value {
            0 => Self::SphinxPacket,
            1 => Self::FinalPayload,
            _ => todo!("return error"),
        }
    }
}

pub async fn write_body<'a, W>(
    writer: &'a mut W,
    body_type: BodyType,
    body: &[u8],
) -> Result<(), Box<dyn Error>>
where
    W: AsyncWrite + Unpin + ?Sized,
{
    writer.write_u8(body_type.as_u8()).await?;
    writer.write_all(body).await?;
    Ok(())
}
