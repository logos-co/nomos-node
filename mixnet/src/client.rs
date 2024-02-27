use std::collections::VecDeque;

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::MixnetError, packet::Packet, poisson::Poisson, topology::MixnetTopology};

/// Mix client implementation that returns packets in Poisson intervals
pub struct MixClient {
    packet_rx: mpsc::UnboundedReceiver<Packet>,
}

struct MixClientRunner {
    config: MixClientConfig,
    poisson: Poisson,
    message_queue: mpsc::Receiver<Box<[u8]>>,
    real_packet_queue: VecDeque<Packet>,
    packet_tx: mpsc::UnboundedSender<Packet>,
}

/// Mix client configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixClientConfig {
    /// Mixnet topology
    pub topology: MixnetTopology,
    /// Poisson rate for packet emissions (per minute)
    pub emission_rate_per_min: f64,
    /// Packet redundancy for passive retransmission
    pub redundancy: u8,
}

const MESSAGE_QUEUE_SIZE: usize = 256;

/// Queue for sending messages to [`MixClient`]
pub type MessageQueue = mpsc::Sender<Box<[u8]>>;

impl MixClient {
    /// Creates a [`MixClient`] and a [`MessageQueue`].
    ///
    /// This returns [`MixnetError`] if the given `config` is invalid.
    pub fn new(config: MixClientConfig) -> Result<(Self, MessageQueue), MixnetError> {
        let poisson = Poisson::new(config.emission_rate_per_min)?;
        let (tx, rx) = mpsc::channel(MESSAGE_QUEUE_SIZE);
        let (packet_tx, packet_rx) = mpsc::unbounded_channel();

        MixClientRunner {
            config,
            poisson,
            message_queue: rx,
            real_packet_queue: VecDeque::new(),
            packet_tx,
        }
        .run();

        Ok((Self { packet_rx }, tx))
    }

    /// Returns a next [`Packet`] to be emitted, if it exists and the Poisson timer is done.
    pub async fn next(&mut self) -> Option<Packet> {
        self.packet_rx.recv().await
    }
}

impl MixClientRunner {
    fn run(mut self) {
        tokio::spawn(async move {
            let mut delay = tokio::time::sleep(self.poisson.interval(&mut OsRng));
            loop {
                let next_deadline = delay.deadline() + self.poisson.interval(&mut OsRng);
                delay.await;
                delay = tokio::time::sleep_until(next_deadline);

                match self.next_packet().await {
                    Ok(packet) => {
                        // packet_tx is always expected to be not closed/dropped.
                        self.packet_tx.send(packet).unwrap();
                    }
                    Err(e) => {
                        tracing::error!(
                            "failed to find a next packet to emit. skipping to the next turn: {e}"
                        );
                    }
                }
            }
        });
    }

    async fn next_packet(&mut self) -> Result<Packet, MixnetError> {
        if let Some(packet) = self.real_packet_queue.pop_front() {
            return Ok(packet);
        }

        match self.message_queue.try_recv() {
            Ok(msg) => {
                for packet in Packet::build_real(&msg, &self.config.topology)? {
                    for _ in 0..self.config.redundancy {
                        self.real_packet_queue.push_back(packet.clone());
                    }
                }
                Ok(self.real_packet_queue.pop_front().unwrap())
            }
            Err(_) => {
                let mut packets =
                    Packet::build_drop_cover("drop cover".as_ref(), &self.config.topology)?;
                assert_eq!(1, packets.len()); // since the cover msg is short
                Ok(packets.pop().unwrap())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::Instant;

    use crate::{
        client::MixClientConfig,
        topology::{
            tests::{gen_entropy, gen_mixnodes},
            MixnetTopology,
        },
    };

    use super::MixClient;

    #[tokio::test]
    async fn poisson_emission() {
        let emission_rate_per_min = 60.0;
        let (mut client, _) = MixClient::new(MixClientConfig {
            topology: MixnetTopology::new(gen_mixnodes(10), 3, 2, gen_entropy()).unwrap(),
            emission_rate_per_min,
            redundancy: 3,
        })
        .unwrap();

        let mut ts = Instant::now();
        let mut intervals = Vec::new();
        for _ in 0..30 {
            assert!(client.next().await.is_some());
            let now = Instant::now();
            intervals.push(now - ts);
            ts = now;
        }

        let avg_sec = intervals.iter().map(|d| d.as_secs()).sum::<u64>() / intervals.len() as u64;
        let expected_avg_sec = (60.0 / emission_rate_per_min) as u64;
        assert!(
            avg_sec.abs_diff(expected_avg_sec) <= 1,
            "{avg_sec} -{expected_avg_sec}"
        );
    }

    #[tokio::test]
    async fn real_packet_emission() {
        let (mut client, msg_queue) = MixClient::new(MixClientConfig {
            topology: MixnetTopology::new(gen_mixnodes(10), 3, 2, gen_entropy()).unwrap(),
            emission_rate_per_min: 360.0,
            redundancy: 3,
        })
        .unwrap();

        msg_queue.send("hello".as_bytes().into()).await.unwrap();

        // Check if the next 3 packets are the same, according to the redundancy
        let packet = client.next().await.unwrap();
        assert_eq!(packet, client.next().await.unwrap());
        assert_eq!(packet, client.next().await.unwrap());

        // Check if the next packet is different (drop cover)
        assert_ne!(packet, client.next().await.unwrap());
    }
}
