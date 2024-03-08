use std::io;

use futures::{AsyncRead, AsyncReadExt, AsyncWrite, AsyncWriteExt};

use crate::{address::NodeAddress, error::MixnetError, topology::MixnetTopology};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Packet {
    address: NodeAddress,
    body: PacketBody,
}

impl Packet {
    pub(crate) fn build_real(
        _msg: &[u8],
        _topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        todo!()
    }

    pub(crate) fn build_drop_cover(
        _msg: &[u8],
        _topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        todo!()
    }

    pub fn address(&self) -> NodeAddress {
        self.address
    }

    pub fn body(self) -> PacketBody {
        self.body
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PacketBody {
    SphinxPacket(Vec<u8>),
    Fragment(Vec<u8>),
}

impl PacketBody {
    pub async fn write_to<W: AsyncWrite + Unpin + ?Sized>(&self, writer: &mut W) -> io::Result<()> {
        match self {
            Self::SphinxPacket(data) => {
                Self::write(writer, PacketBodyFlag::SphinxPacket, data).await
            }
            Self::Fragment(data) => Self::write(writer, PacketBodyFlag::Fragment, data).await,
        }
    }
    async fn write<W: AsyncWrite + Unpin + ?Sized>(
        writer: &mut W,
        flag: PacketBodyFlag,
        data: &[u8],
    ) -> io::Result<()> {
        writer.write_all(&[flag as u8]).await?;
        writer.write_all(&data.len().to_le_bytes()).await?;
        writer.write_all(data).await?;
        Ok(())
    }

    pub async fn read_from<R: AsyncRead + Unpin>(
        reader: &mut R,
    ) -> io::Result<Result<Self, MixnetError>> {
        let mut flag = [0u8; 1];
        reader.read_exact(&mut flag).await?;

        let mut size = [0u8; std::mem::size_of::<usize>()];
        reader.read_exact(&mut size).await?;

        let mut data = vec![0u8; usize::from_le_bytes(size)];
        reader.read_exact(&mut data).await?;

        match PacketBodyFlag::try_from(flag[0]) {
            Ok(PacketBodyFlag::SphinxPacket) => Ok(Ok(PacketBody::SphinxPacket(data))),
            Ok(PacketBodyFlag::Fragment) => Ok(Ok(PacketBody::Fragment(data))),
            Err(e) => Ok(Err(e)),
        }
    }
}

#[repr(u8)]
enum PacketBodyFlag {
    SphinxPacket,
    Fragment,
}

impl TryFrom<u8> for PacketBodyFlag {
    type Error = MixnetError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0u8 => Ok(PacketBodyFlag::SphinxPacket),
            1u8 => Ok(PacketBodyFlag::Fragment),
            _ => Err(MixnetError::InvalidPacketFlag),
        }
    }
}

#[allow(dead_code)]
pub(crate) enum Message<'a> {
    Real(&'a [u8]),
    DropCover(&'a [u8]),
}

impl<'a> Message<'a> {
    #[allow(dead_code)]
    pub(crate) fn bytes(&self) -> Box<[u8]> {
        //TODO: optimize memcpy
        match self {
            Message::Real(value) => MessageFlag::Real.set(value),
            Message::DropCover(value) => MessageFlag::DropCover.set(value),
        }
    }

    #[allow(dead_code)]
    pub(crate) fn from_bytes(value: &'a [u8]) -> Result<Self, MixnetError> {
        match MessageFlag::try_from(value[0])? {
            MessageFlag::Real => Ok(Self::Real(value[1..].into())),
            MessageFlag::DropCover => Ok(Self::DropCover(value[1..].into())),
        }
    }
}

#[repr(u8)]
enum MessageFlag {
    Real,
    DropCover,
}

impl MessageFlag {
    fn set(self, body: &[u8]) -> Box<[u8]> {
        let mut out = Vec::with_capacity(1 + body.len());
        out.push(self as u8);
        out.extend_from_slice(body);
        out.into_boxed_slice()
    }
}

impl TryFrom<u8> for MessageFlag {
    type Error = MixnetError;

    fn try_from(value: u8) -> Result<Self, Self::Error> {
        match value {
            0u8 => Ok(MessageFlag::Real),
            1u8 => Ok(MessageFlag::DropCover),
            _ => Err(MixnetError::InvalidPacketFlag),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn message_flag() {
        assert_eq!(&[0, 1, 2], MessageFlag::Real.set(&[1, 2]).as_ref());
        assert_eq!(&[1, 1, 2], MessageFlag::DropCover.set(&[1, 2]).as_ref());
    }
}
