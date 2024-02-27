use crate::{address::NodeAddress, error::MixnetError};

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Packet {
    address: NodeAddress,
    body: PacketBody,
}

impl Packet {
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
