use std::{
    collections::VecDeque,
    num::NonZeroU8,
    pin::Pin,
    task::{Context, Poll},
};

use futures::{Future, Stream};
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
    config: MixClientConfig,
    poisson: Poisson,
    message_queue: mpsc::Receiver<Vec<u8>>,
    real_packet_queue: VecDeque<Packet>,
    delay: Option<Pin<Box<tokio::time::Sleep>>>,
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
pub type MessageQueue = mpsc::Sender<Vec<u8>>;

impl MixClient {
    /// Creates a [`MixClient`] and a [`MessageQueue`].
    ///
    /// This returns [`MixnetError`] if the given `config` is invalid.
    pub fn new(config: MixClientConfig) -> Result<(Self, MessageQueue), MixnetError> {
        let poisson = Poisson::new(config.emission_rate_per_min)?;
        let (tx, rx) = mpsc::channel(MESSAGE_QUEUE_SIZE);

        Ok((
            Self {
                config,
                poisson,
                message_queue: rx,
                real_packet_queue: VecDeque::new(),
                delay: None,
            },
            tx,
        ))
    }
}

impl Stream for MixClient {
    type Item = Packet;

    /// Returns a next [`Packet`] to be emitted to the next, if it exists and the Poisson timer is done.
    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if self.delay.is_none() {
            // We've never set an initial delay. Let's do it now.
            cx.waker().wake_by_ref();

            self.delay = Some(Box::pin(tokio::time::sleep(
                self.poisson.interval(&mut OsRng),
            )));
            return Poll::Pending;
        }

        match self.delay.as_mut().unwrap().as_mut().poll(cx) {
            Poll::Pending => {
                // The delay hasn't elapsed yet.
                // The current task is automatically scheduled to be woken up once the timer elapses,
                // thanks to the `tokio::time::Sleep.poll(cx)`.
                Poll::Pending
            }
            Poll::Ready(_) => {
                // The delay has elapsed. Let's reset the delay and return the next packet.
                let next_interval = self.poisson.interval(&mut OsRng);
                let delay = self.delay.as_mut().unwrap();
                let next_deadline = delay.deadline() + next_interval;
                delay.as_mut().reset(next_deadline);

                match self.next_packet() {
                    Ok(packet) => Poll::Ready(Some(packet)),
                    Err(e) => {
                        tracing::error!(
                            "failed to find a next packet to emit. skipping to the next turn: {e}"
                        );
                        Poll::Pending
                    }
                }
            }
        }
    }
}

impl MixClient {
    const DROP_COVER_MSG: &'static [u8] = b"drop cover";

    // Returns either a real packet or a drop cover packet.
    fn next_packet(&mut self) -> Result<Packet, MixnetError> {
        // If there is any redundant real packet scheduled, return it.
        if let Some(packet) = self.real_packet_queue.pop_front() {
            return Ok(packet);
        }

        match self.message_queue.try_recv() {
            Ok(msg) => {
                // If there is any message received, build real packets out of it and
                // schedule them in the queue.
                for packet in Packet::build_real(msg, &self.config.topology)? {
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
                // If no message received, generate and return a drop cover packet.
                let mut packets = Packet::build_drop_cover(
                    Vec::from(Self::DROP_COVER_MSG),
                    &self.config.topology,
                )?;
                Ok(packets.pop().expect("drop cover should not be empty"))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::{num::NonZeroU8, time::Instant};

    use futures::StreamExt;

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
            redundancy: NonZeroU8::new(3).unwrap(),
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
            redundancy: NonZeroU8::new(3).unwrap(),
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
