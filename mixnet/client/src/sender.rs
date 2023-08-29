use std::{error::Error, net::SocketAddr, time::Duration};

use mixnet_protocol::Body;
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

    pub fn send(&mut self, msg: Vec<u8>, total_delay: Duration) -> Result<(), Box<dyn Error>> {
        let destination = self.topology.random_destination(&mut self.rng)?;
        let destination = Destination::new(
            DestinationAddressBytes::from_bytes(destination.address.as_bytes()),
            [0; IDENTIFIER_LENGTH], // TODO: use a proper SURBIdentifier if we need SURB
        );

        self.pad_and_split_message(msg)
            .into_iter()
            .map(|fragment| self.build_sphinx_packet(fragment, &destination, total_delay))
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
        total_delay: Duration,
    ) -> Result<(sphinx_packet::SphinxPacket, route::Node), Box<dyn Error>> {
        let route = self.topology.random_route(&mut self.rng)?;

        let delays: Vec<Delay> = RandomDelayIterator::new(
            &mut self.rng,
            route.len() as u64,
            total_delay.as_millis() as u64,
        )
        .map(Delay::new_from_millis)
        .collect();

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
        let body = Body::new_sphinx(packet);
        body.write(&mut socket).await?;
        tracing::debug!("Sent a Sphinx packet successuflly to the node: {addr:?}");

        Ok(())
    }
}

struct RandomDelayIterator<R> {
    rng: R,
    remaining_delays: u64,
    remaining_time: u64,
    avg_delay: u64,
}

impl<R> RandomDelayIterator<R> {
    fn new(rng: R, total_delays: u64, total_time: u64) -> Self {
        RandomDelayIterator {
            rng,
            remaining_delays: total_delays,
            remaining_time: total_time,
            avg_delay: total_time / total_delays,
        }
    }
}

impl<R> Iterator for RandomDelayIterator<R>
where
    R: Rng,
{
    type Item = u64;

    fn next(&mut self) -> Option<u64> {
        if self.remaining_delays == 0 {
            return None;
        }

        self.remaining_delays -= 1;

        // corner case: 0 average delay
        if self.avg_delay == 0 {
            return Some(0);
        }

        if self.remaining_delays == 1 {
            return Some(self.remaining_time);
        }

        // if we have no remaining time, then just return 0
        if self.remaining_time == 0 {
            return Some(0);
        }

        // Calculate bounds to avoid extreme values
        let upper_bound = (self.avg_delay as f64 * 1.5)
            // guarantee that we don't exceed the remaining time and promise the delay we return is
            // at least 1ms.
            .min((self.remaining_time as f64 - self.remaining_delays as f64) + 1.0);
        let lower_bound = (self.avg_delay as f64 * 0.5).min(upper_bound);

        let delay = self.rng.gen_range(lower_bound, upper_bound + 1.0).floor() as u64;
        self.remaining_time = self.remaining_time.saturating_sub(delay);

        Some(delay)
    }
}

#[cfg(test)]
mod tests {
    use super::RandomDelayIterator;

    const TOTAL_DELAYS: u64 = 3;

    #[test]
    fn test_random_delay_iter_zero_total_time() {
        let mut delays = RandomDelayIterator::new(rand::thread_rng(), TOTAL_DELAYS, 0);
        for _ in 0..TOTAL_DELAYS {
            assert_eq!(delays.next(), Some(0));
        }
        assert!(delays.next().is_none());
    }

    #[test]
    fn test_random_delay_iter_small_total_time() {
        const TOTAL_DELAYS: u64 = 3;
        let mut delays = RandomDelayIterator::new(rand::thread_rng(), TOTAL_DELAYS, 1);
        for _ in 0..TOTAL_DELAYS {
            assert!(delays.next().is_some());
        }
        assert!(delays.next().is_none());
    }
}
