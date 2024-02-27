use sphinx_packet::header::delays::Delay;

use crate::address::NodeAddress;
use crate::crypto::PrivateKey;
use crate::error::MixnetError;
use crate::fragment::{Fragment, FragmentSet};
use crate::topology::MixnetTopology;

#[derive(Clone, PartialEq, Eq, Debug)]
pub struct Packet {
    address: NodeAddress,
    body: PacketBody,
}

impl Packet {
    fn new(processed_packet: sphinx_packet::ProcessedPacket) -> Result<Self, MixnetError> {
        match processed_packet {
            sphinx_packet::ProcessedPacket::ForwardHop(packet, addr, _) => Ok(Packet {
                address: addr.try_into()?,
                body: PacketBody::from(packet.as_ref()),
            }),
            sphinx_packet::ProcessedPacket::FinalHop(addr, _, payload) => Ok(Packet {
                address: addr.try_into()?,
                body: PacketBody::try_from(payload)?,
            }),
        }
    }

    pub(crate) fn build_real(
        msg: &[u8],
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        Self::build(Message::Real(msg), topology)
    }

    pub(crate) fn build_drop_cover(
        msg: &[u8],
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        Self::build(Message::DropCover(msg), topology)
    }

    fn build(msg: Message, topology: &MixnetTopology) -> Result<Vec<Packet>, MixnetError> {
        let destination = topology.choose_destination();

        let fragment_set = FragmentSet::new(&msg.bytes())?;
        let mut packets = Vec::with_capacity(fragment_set.len());
        for fragment in fragment_set.iter() {
            let route = topology.gen_route();
            if route.is_empty() {
                // Create a packet that will be directly sent to the mix destination
                packets.push(Packet {
                    address: NodeAddress::try_from(destination.address)?,
                    body: PacketBody::from(fragment),
                });
            } else {
                // Use dummy delays because mixnodes will ignore this value and generate delay randomly by themselves.
                let delays = vec![Delay::new_from_nanos(0); route.len()];
                packets.push(Packet {
                    address: NodeAddress::try_from(route[0].address)?,
                    body: PacketBody::from(&sphinx_packet::SphinxPacket::new(
                        fragment.bytes(),
                        &route,
                        &destination,
                        &delays,
                    )?),
                });
            }
        }
        Ok(packets)
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

impl From<&sphinx_packet::SphinxPacket> for PacketBody {
    fn from(packet: &sphinx_packet::SphinxPacket) -> Self {
        Self::SphinxPacket(packet.to_bytes().into_boxed_slice())
    }
}

impl From<&Fragment> for PacketBody {
    fn from(fragment: &Fragment) -> Self {
        Self::Fragment(fragment.bytes().into_boxed_slice())
    }
}

impl TryFrom<sphinx_packet::payload::Payload> for PacketBody {
    type Error = MixnetError;

    fn try_from(payload: sphinx_packet::payload::Payload) -> Result<Self, Self::Error> {
        Ok(Self::Fragment(
            payload.recover_plaintext()?.into_boxed_slice(),
        ))
    }
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

    pub(crate) fn process_sphinx_packet(
        packet: &[u8],
        private_key: &PrivateKey,
    ) -> Result<Packet, MixnetError> {
        Packet::new(sphinx_packet::SphinxPacket::from_bytes(packet)?.process(private_key)?)
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
