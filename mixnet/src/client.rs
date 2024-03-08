use std::{collections::VecDeque, num::NonZeroU8};

use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use tokio::sync::mpsc;

use crate::{error::MixnetError, packet::Packet, poisson::Poisson, topology::MixnetTopology};

/// Mix client implementation that is used to schedule messages to be sent to the mixnet.
/// Messages inserted to the [`MessageQueue`] are scheduled according to the Poisson interals
/// and returns from [`MixClient.next()`] when it is ready to be sent to the mixnet.
/// If there is no messages inserted to the [`MessageQueue`], cover packets are generated and
/// returned from [`MixClient.next()`].
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
    pub redundancy: NonZeroU8,
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
                    for _ in 0..self.config.redundancy.get() {
                        self.real_packet_queue.push_back(packet.clone());
                    }
                }
                Ok(self
                    .real_packet_queue
                    .pop_front()
                    .expect("real packet queue should not be empty"))
            }
            Err(_) => {
                let mut packets =
                    Packet::build_drop_cover("drop cover".as_ref(), &self.config.topology)?;
                Ok(packets.pop().expect("drop cover should not be empty"))
            }
        }
    }
}