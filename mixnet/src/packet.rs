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

    pub fn body(self) -> Box<[u8]> {
        self.body.bytes()
    }
}

#[derive(Clone, PartialEq, Eq, Debug)]
pub(crate) enum PacketBody {
    SphinxPacket(Vec<u8>),
    Fragment(Vec<u8>),
}

impl PacketBody {
    pub(crate) fn bytes(self) -> Box<[u8]> {
        match self {
            Self::SphinxPacket(data) => Self::bytes_with_flag(PacketBodyFlag::SphinxPacket, data),
            Self::Fragment(data) => Self::bytes_with_flag(PacketBodyFlag::Fragment, data),
        }
    }

    fn bytes_with_flag(flag: PacketBodyFlag, mut data: Vec<u8>) -> Box<[u8]> {
        let mut out = Vec::with_capacity(1 + data.len());
        out.push(flag as u8);
        out.append(&mut data);
        out.into_boxed_slice()
    }

    pub(crate) fn from_bytes(mut value: Vec<u8>) -> Result<Self, MixnetError> {
        if value.len() < 1 {
            return Err(MixnetError::InvalidPacketBody);
        }
        match PacketBodyFlag::try_from(value[0])? {
            PacketBodyFlag::SphinxPacket => Ok(Self::SphinxPacket(value.split_off(1))),
            PacketBodyFlag::Fragment => Ok(Self::Fragment(value.split_off(1))),
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
