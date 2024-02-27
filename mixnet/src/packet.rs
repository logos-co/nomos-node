use crate::address::NodeAddress;
use crate::error::MixnetError;
use crate::topology::MixnetTopology;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Packet {
    address: NodeAddress,
    body: PacketBody,
}

impl Packet {
    pub(crate) fn build_real(
        msg: &[u8],
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        todo!()
    }

    pub(crate) fn build_drop_cover(
        msg: &[u8],
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        todo!()
    }

    pub fn address(&self) -> NodeAddress {
        self.address
    }

    pub fn body(&self) -> Box<[u8]> {
        self.body.bytes()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum PacketBody {
    SphinxPacket(Box<[u8]>),
    Fragment(Box<[u8]>),
}

impl PacketBody {
    pub(crate) fn bytes(&self) -> Box<[u8]> {
        match self {
            Self::SphinxPacket(data) => PacketBodyFlag::SphinxPacket.set(data),
            Self::Fragment(data) => PacketBodyFlag::Fragment.set(data),
        }
    }

    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        match PacketBodyFlag::try_from(value[0])? {
            PacketBodyFlag::SphinxPacket => Ok(Self::SphinxPacket(value[1..].into())),
            PacketBodyFlag::Fragment => Ok(Self::Fragment(value[1..].into())),
        }
    }
}

#[repr(u8)]
enum PacketBodyFlag {
    SphinxPacket,
    Fragment,
}

impl PacketBodyFlag {
    fn set(self, body: &[u8]) -> Box<[u8]> {
        let mut out = Vec::with_capacity(1 + body.len());
        out.push(self as u8);
        out.extend_from_slice(body);
        out.into_boxed_slice()
    }
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

pub(crate) enum Message<'a> {
    Real(&'a [u8]),
    DropCover(&'a [u8]),
}

impl<'a> Message<'a> {
    pub(crate) fn bytes(&self) -> Box<[u8]> {
        //TODO: optimize memcpy
        match self {
            Message::Real(value) => MessageFlag::Real.set(value),
            Message::DropCover(value) => MessageFlag::DropCover.set(value),
        }
    }

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
