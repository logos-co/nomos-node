use std::{
    collections::{HashMap, HashSet},
    error::Error,
    io::ErrorKind,
    net::SocketAddr,
    sync::Arc,
    time::Duration,
};

use mixnet_protocol::{Body, PacketId};
use mixnet_topology::MixnetTopology;
use mixnet_util::ConnectionPool;
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, chunking::fragment::Fragment, message::NymMessage,
    params::PacketSize, Delay, Destination, DestinationAddressBytes, NodeAddressBytes,
    IDENTIFIER_LENGTH, PAYLOAD_OVERHEAD_SIZE,
};
use rand::{distributions::Uniform, prelude::Distribution, Rng};
use sphinx_packet::{route, SphinxPacket, SphinxPacketBuilder};
use tokio::{net::TcpStream, sync::Mutex};

// Sender splits messages into Sphinx packets and sends them to the Mixnet.
pub struct Sender<R: Rng> {
    //TODO: handle topology update
    topology: MixnetTopology,
    pool: ConnectionPool,
    ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
    max_retries: usize,
    retry_delay: Duration,
    rng: R,
}

impl<R: Rng> Sender<R> {
    pub fn new(
        topology: MixnetTopology,
        pool: ConnectionPool,
        rng: R,
        max_retries: usize,
        retry_delay: Duration,
    ) -> Self {
        Self {
            topology,
            rng,
            pool,
            ack_cache: Arc::new(Mutex::new(HashMap::new())),
            max_retries,
            retry_delay,
        }
    }

    pub fn send(
        &mut self,
        msg: Vec<u8>,
        total_delay: Duration,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
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
                let pool = self.pool.clone();
                let ack_cache = self.ack_cache.clone();
                let max_retries = self.max_retries;
                let retry_delay = self.retry_delay;
                tokio::spawn(async move {
                    if let Err(e) = Self::send_packet(
                        &pool,
                        ack_cache,
                        max_retries,
                        retry_delay,
                        Box::new(packet),
                        first_node.address,
                    )
                    .await
                    {
                        tracing::error!("failed to send packet to the first node: {e}");
                    }
                });
            });

        Ok(())
    }

    fn pad_and_split_message(&mut self, msg: Vec<u8>) -> Vec<Fragment> {
        let nym_message = NymMessage::new_plain(msg);

        // TODO: add PUBLIC_KEY_SIZE for encryption for the destination,
        //       if we're going to encrypt final payloads for the destination.
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
    ) -> Result<(sphinx_packet::SphinxPacket, route::Node), Box<dyn Error + Send + Sync + 'static>>
    {
        let route = self.topology.random_route(&mut self.rng)?;

        let delays: Vec<Delay> =
            RandomDelayIterator::new(&mut self.rng, route.len() as u64, total_delay)
                .map(|d| Delay::new_from_millis(d.as_millis() as u64))
                .collect();

        // TODO: encrypt the payload for the destination, if we want
        // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/src/preparer/payload.rs#L70
        let payload = fragment.into_bytes();

        let packet = SphinxPacketBuilder::new()
            .with_payload_size(payload.len() + PAYLOAD_OVERHEAD_SIZE)
            .build_packet(payload, &route, destination, &delays)?;

        let first_mixnode = route.first().cloned().expect("route is not empty");

        Ok((packet, first_mixnode))
    }

    async fn send_packet(
        pool: &ConnectionPool,
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        packet: Box<SphinxPacket>,
        addr: NodeAddressBytes,
    ) -> Result<(), Box<dyn Error + Send + Sync + 'static>> {
        let addr = SocketAddr::try_from(NymNodeRoutingAddress::try_from(addr)?)?;
        tracing::debug!("Sending a Sphinx packet to the node: {addr:?}");

        let mu: std::sync::Arc<tokio::sync::Mutex<tokio::net::TcpStream>> =
            pool.get_or_init(&addr).await?;
        let arc_socket = mu.clone();
        let mut socket = mu.lock().await;
        let bytes = packet.to_bytes();
        let packet_id = PacketId::from_bytes(&bytes);
        match Body::write_sphinx_packet_bytes(&mut *socket, &bytes).await {
            Ok(_) => {
                tracing::info!("Sent a Sphinx packet successuflly to the node: {addr}");
                Self::insert_packet_id(&ack_cache, addr, packet_id).await;

                tokio::spawn(async move {
                    Self::retry_backoff(
                        ack_cache,
                        max_retries,
                        retry_delay,
                        packet_id,
                        bytes,
                        addr,
                        arc_socket,
                    )
                    .await;
                });
                Ok(())
            }
            Err(e) => {
                if let Some(err) = e.downcast_ref::<std::io::Error>() {
                    match err.kind() {
                        ErrorKind::BrokenPipe
                        | ErrorKind::NotConnected
                        | ErrorKind::ConnectionAborted => {
                            tracing::warn!("broken pipe error while sending a Sphinx packet to the node: {addr}, try to update the connection and retry");
                            // update the connection
                            let mut tcp = TcpStream::connect(addr).await?;
                            // resend packet immediately
                            Body::write_sphinx_packet_bytes(&mut tcp, &bytes).await?;
                            *socket = tcp;
                            tracing::info!("Sent a Sphinx packet successuflly to the node: {addr}");
                            Self::insert_packet_id(&ack_cache, addr, packet_id).await;
                            tokio::spawn(async move {
                                Self::retry_backoff(
                                    ack_cache,
                                    max_retries,
                                    retry_delay,
                                    packet_id,
                                    bytes,
                                    addr,
                                    arc_socket,
                                )
                                .await;
                            });
                            Ok(())
                        }
                        _ => Err(e),
                    }
                } else {
                    Err(e)
                }
            }
        }
    }

    async fn insert_packet_id(
        ack_cache: &Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        addr: SocketAddr,
        packet_id: PacketId,
    ) {
        ack_cache
            .lock()
            .await
            .entry(addr)
            .or_insert_with(HashSet::new)
            .insert(packet_id);
    }

    async fn retry_backoff(
        ack_cache: Arc<Mutex<HashMap<SocketAddr, HashSet<PacketId>>>>,
        max_retries: usize,
        retry_delay: Duration,
        packet_id: PacketId,
        sphinx_bytes: Vec<u8>,
        peer_addr: SocketAddr,
        socket: Arc<Mutex<TcpStream>>,
    ) {
        for _ in 0..max_retries {
            tokio::time::sleep(retry_delay).await;
            let mu = ack_cache.lock().await;
            if let Some(ids) = mu.get(&peer_addr) {
                if ids.contains(&packet_id) {
                    tracing::debug!("retrying to send a Sphinx packet to the peer {peer_addr}");
                    let mut socket = socket.lock().await;
                    match Body::write_sphinx_packet_bytes(&mut *socket, &sphinx_bytes).await {
                        Ok(_) => {
                            tracing::info!(
                                "Sent a Sphinx packet successuflly to the node: {peer_addr}"
                            );
                            return;
                        }
                        Err(e) => {
                            if let Some(err) = e.downcast_ref::<std::io::Error>() {
                                match err.kind() {
                                    ErrorKind::BrokenPipe
                                    | ErrorKind::NotConnected
                                    | ErrorKind::ConnectionAborted => {
                                        tracing::warn!("broken pipe error while sending a Sphinx packet to the node: {peer_addr}, try to update the connection and retry");
                                        // update the connection
                                        match TcpStream::connect(peer_addr).await {
                                            Ok(tcp) => {
                                                *socket = tcp;
                                            }
                                            Err(e) => {
                                                tracing::error!("failed to update the connection to the node: {peer_addr}, error: {e}");
                                            }
                                        }
                                    }
                                    _ => {}
                                }
                            }
                        }
                    }
                } else {
                    return;
                }
            }
        }
    }
}

struct RandomDelayIterator<R> {
    rng: R,
    remaining_delays: u64,
    remaining_time: u64,
    avg_delay: u64,
}

impl<R> RandomDelayIterator<R> {
    fn new(rng: R, total_delays: u64, total_time: Duration) -> Self {
        let total_time = total_time.as_millis() as u64;
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
    type Item = Duration;

    fn next(&mut self) -> Option<Duration> {
        if self.remaining_delays == 0 {
            return None;
        }

        self.remaining_delays -= 1;

        if self.remaining_delays == 1 {
            return Some(Duration::from_millis(self.remaining_time));
        }

        // Calculate bounds to avoid extreme values
        let upper_bound = (self.avg_delay as f64 * 1.5)
            // guarantee that we don't exceed the remaining time and promise the delay we return is
            // at least 1ms.
            .min(self.remaining_time.saturating_sub(self.remaining_delays) as f64);
        let lower_bound = (self.avg_delay as f64 * 0.5).min(upper_bound);

        let delay = Uniform::new_inclusive(lower_bound, upper_bound).sample(&mut self.rng) as u64;
        self.remaining_time = self.remaining_time.saturating_sub(delay);

        Some(Duration::from_millis(delay))
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::RandomDelayIterator;

    const TOTAL_DELAYS: u64 = 3;

    #[test]
    fn test_random_delay_iter_zero_total_time() {
        let mut delays = RandomDelayIterator::new(rand::thread_rng(), TOTAL_DELAYS, Duration::ZERO);
        for _ in 0..TOTAL_DELAYS {
            assert!(delays.next().is_some());
        }
        assert!(delays.next().is_none());
    }

    #[test]
    fn test_random_delay_iter_small_total_time() {
        let mut delays =
            RandomDelayIterator::new(rand::thread_rng(), TOTAL_DELAYS, Duration::from_millis(1));
        let mut d = Duration::ZERO;
        for _ in 0..TOTAL_DELAYS {
            d += delays.next().unwrap();
        }
        assert!(delays.next().is_none());
        assert_eq!(d, Duration::from_millis(1));
    }
}
