use std::{error::Error, net::SocketAddr};

use mixnet_protocol::{write_body, BodyType};
use mixnet_topology::MixnetTopology;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, chunking::fragment::Fragment, message::NymMessage,
    params::PacketSize, Delay, Destination, DestinationAddressBytes, NodeAddressBytes,
    IDENTIFIER_LENGTH, PAYLOAD_OVERHEAD_SIZE,
};
use rand::Rng;
use sphinx_packet::{route, SphinxPacket, SphinxPacketBuilder};
use tokio::net::TcpStream;

// Sender splits messages into Sphinx packets and sends them to the Mixnet.
pub struct Sender<R: Rng> {
    //TODO: handle topology update
    topology: MixnetTopology,
    rng: R,
}

impl<R: Rng> Sender<R> {
    pub fn new(topology: MixnetTopology, rng: R) -> Self {
        Self { topology, rng }
    }

    pub fn send(&mut self, msg: Vec<u8>) -> Result<(), Box<dyn Error>> {
        let destination = self.topology.random_destination(&mut self.rng)?;
        let destination = Destination::new(
            DestinationAddressBytes::from_bytes(destination.address.as_bytes()),
            [0; IDENTIFIER_LENGTH], // TODO: use a proper SURBIdentifier if we need SURB
        );

        self.pad_and_split_message(msg)
            .into_iter()
            .map(|fragment| self.build_sphinx_packet(fragment, &destination))
            .collect::<Result<Vec<_>, _>>()?
            .into_iter()
            .for_each(|(packet, first_node)| {
                tokio::spawn(async move {
                    if let Err(e) = Self::send_packet(Box::new(packet), first_node.address).await {
                        tracing::error!("failed to send packet to the first node: {e}");
                    }
                });
            });

        Ok(())
    }

    fn pad_and_split_message(&mut self, msg: Vec<u8>) -> Vec<Fragment> {
        let nym_message = NymMessage::new_plain(msg);

        // TODO: add PUBLIC_KEY_SIZE for encryption for the destination
        // TODO: add ACK_OVERHEAD if we need SURB-ACKs.
        // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/src/message.rs#L181-L181
        let plaintext_size_per_packet = PacketSize::RegularPacket.plaintext_size();

        nym_message
            .pad_to_full_packet_lengths(plaintext_size_per_packet)
            .split_into_fragments(&mut self.rng, plaintext_size_per_packet)
    }

    fn build_sphinx_packet(
        &mut self,
        fragment: Fragment,
        destination: &Destination,
    ) -> Result<(sphinx_packet::SphinxPacket, route::Node), Box<dyn Error>> {
        let route = self.topology.random_route(&mut self.rng)?;

        // TODO: use proper delays
        let delays: Vec<Delay> = route.iter().map(|_| Delay::new_from_nanos(0)).collect();

        // TODO: encryption for the destination
        // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/src/preparer/payload.rs#L70
        let payload = fragment.into_bytes();

        let packet = SphinxPacketBuilder::new()
            .with_payload_size(payload.len() + PAYLOAD_OVERHEAD_SIZE)
            .build_packet(payload, &route, destination, &delays)?;

        let first_mixnode = route.first().cloned().expect("route is not empty");

        Ok((packet, first_mixnode))
    }

    async fn send_packet(
        packet: Box<SphinxPacket>,
        addr: NodeAddressBytes,
    ) -> Result<(), Box<dyn Error>> {
        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from(addr)?)?;
        tracing::debug!("Sending a Sphinx packet to the node: {addr:?}");

        let mut socket = TcpStream::connect(addr).await?;
        write_body(&mut socket, BodyType::SphinxPacket, &packet.to_bytes()).await?;
        tracing::debug!("Sent a Sphinx packet successuflly to the node: {addr:?}");

        Ok(())
    }
}
