use std::io;

use futures::{AsyncRead, AsyncReadExt};
use sphinx_packet::{crypto::PrivateKey, header::delays::Delay};

use crate::{
    address::NodeAddress,
    error::MixnetError,
    fragment::{Fragment, FragmentSet},
    topology::MixnetTopology,
};

/// A packet to be sent through the mixnet
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
        msg: Vec<u8>,
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        Self::build(Message::Real(msg), topology)
    }

    pub(crate) fn build_drop_cover(
        msg: Vec<u8>,
        topology: &MixnetTopology,
    ) -> Result<Vec<Packet>, MixnetError> {
        Self::build(Message::DropCover(msg), topology)
    }

    fn build(msg: Message, topology: &MixnetTopology) -> Result<Vec<Packet>, MixnetError> {
        let destination = topology.choose_destination();

        let fragment_set = FragmentSet::new(&msg.bytes())?;
        let mut packets = Vec::with_capacity(fragment_set.as_ref().len());
        for fragment in fragment_set.as_ref().iter() {
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

    /// Returns the address of the mix node that this packet is being sent to
    pub fn address(&self) -> NodeAddress {
        self.address
    }

    /// Returns the body of the packet
    pub fn body(self) -> PacketBody {
        self.body
    }
}

/// The body of a packet to be sent through the mixnet
#[derive(Clone, PartialEq, Eq, Debug)]
pub enum PacketBody {
    /// A Sphinx packet to be sent to the next mix node
    SphinxPacket(Vec<u8>),
    /// A fragment that has been through the mixnet and can be reconstructed into the original message
    Fragment(Vec<u8>),
}

impl From<&sphinx_packet::SphinxPacket> for PacketBody {
    fn from(packet: &sphinx_packet::SphinxPacket) -> Self {
        Self::SphinxPacket(packet.to_bytes())
    }
}

impl From<&Fragment> for PacketBody {
    fn from(fragment: &Fragment) -> Self {
        Self::Fragment(fragment.bytes())
    }
}

impl TryFrom<sphinx_packet::payload::Payload> for PacketBody {
    type Error = MixnetError;

    fn try_from(payload: sphinx_packet::payload::Payload) -> Result<Self, Self::Error> {
        Ok(Self::Fragment(payload.recover_plaintext()?))
    }
}

impl PacketBody {
    /// Consumes the packet body and serialize it into a byte array
    pub fn bytes(self) -> Box<[u8]> {
        match self {
            Self::SphinxPacket(data) => Self::bytes_with_flag(PacketBodyFlag::SphinxPacket, data),
            Self::Fragment(data) => Self::bytes_with_flag(PacketBodyFlag::Fragment, data),
        }
    }

    fn bytes_with_flag(flag: PacketBodyFlag, mut msg: Vec<u8>) -> Box<[u8]> {
        let mut out = Vec::with_capacity(1 + std::mem::size_of::<usize>() + msg.len());
        out.push(flag as u8);
        out.extend_from_slice(&msg.len().to_le_bytes());
        out.append(&mut msg);
        out.into_boxed_slice()
    }

    /// Deserialize a packet body from a reader
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

pub(crate) enum Message {
    Real(Vec<u8>),
    DropCover(Vec<u8>),
}

impl Message {
    fn bytes(self) -> Box<[u8]> {
        match self {
            Self::Real(msg) => Self::bytes_with_flag(MessageFlag::Real, msg),
            Self::DropCover(msg) => Self::bytes_with_flag(MessageFlag::DropCover, msg),
        }
    }

    fn bytes_with_flag(flag: MessageFlag, mut msg: Vec<u8>) -> Box<[u8]> {
        let mut out = Vec::with_capacity(1 + msg.len());
        out.push(flag as u8);
        out.append(&mut msg);
        out.into_boxed_slice()
    }

    pub(crate) fn from_bytes(value: &[u8]) -> Result<Self, MixnetError> {
        if value.is_empty() {
            return Err(MixnetError::InvalidMessage);
        }

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
