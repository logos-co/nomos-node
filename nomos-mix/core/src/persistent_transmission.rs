use futures::Stream;
use nomos_mix_message::DROP_MESSAGE;
use rand::{distributions::Uniform, prelude::Distribution, Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use serde::{Deserialize, Serialize};
use std::collections::VecDeque;
use std::pin::{pin, Pin};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time::Interval;
use tokio::{
    sync::mpsc::{self, error::TryRecvError},
    time,
};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PersistentTransmissionSettings {
    /// The maximum number of messages that can be emitted per second
    pub max_emission_frequency: f64,
    /// The probability of emitting a drop message by coin flipping
    pub drop_message_probability: f64,
}

impl Default for PersistentTransmissionSettings {
    fn default() -> Self {
        Self {
            max_emission_frequency: 1.0,
            drop_message_probability: 0.5,
        }
    }
}

pub struct PersistentTransmissionStream<S>
where
    S: Stream,
{
    interval: Interval,
    coin: Coin<ChaCha12Rng>,
    queue: VecDeque<S::Item>,
    stream: S,
}

impl<S> PersistentTransmissionStream<S>
where
    S: Stream,
{
    pub fn new(
        settings: PersistentTransmissionSettings,
        stream: S,
    ) -> PersistentTransmissionStream<S> {
        let interval = time::interval(Duration::from_secs_f64(
            1.0 / settings.max_emission_frequency,
        ));
        let coin = Coin::<_>::new(
            ChaCha12Rng::from_entropy(),
            settings.drop_message_probability,
        )
        .unwrap();
        Self {
            interval,
            coin,
            queue: Default::default(),
            stream,
        }
    }
}

impl<S> Stream for PersistentTransmissionStream<S>
where
    S: Stream<Item = Vec<u8>> + Unpin,
{
    type Item = S::Item;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self {
            ref mut interval,
            ref mut queue,
            ref mut stream,
            ref mut coin,
            ..
        } = self.get_mut();
        if pin!(interval).poll_tick(cx).is_ready() {
            match queue.pop_front() {
                item @ Some(_) => return Poll::Ready(item),
                None => {
                    if coin.flip() {
                        return Poll::Ready(Some(DROP_MESSAGE.to_vec()));
                    }
                }
            };
        };
        if let Poll::Ready(Some(item)) = pin!(stream).poll_next(cx) {
            queue.push_back(item);
        }
        Poll::Pending
    }
}

pub trait PersistentTransmission: Stream {
    fn persistent_transmission(
        self,
        settings: PersistentTransmissionSettings,
    ) -> PersistentTransmissionStream<Self>
    where
        Self: Sized + Unpin,
    {
        PersistentTransmissionStream::new(settings, self)
    }
}

impl<S> PersistentTransmission for S where S: Stream {}

/// Transmit scheduled messages with a persistent rate to the transmission channel.
///
/// # Arguments
///
/// * `settings` - The settings for the persistent transmission
/// * `schedule_receiver` - The channel for messages scheduled (from Tier 2 currently)
/// * `emission_sender` - The channel to emit messages
pub async fn persistent_transmission(
    settings: PersistentTransmissionSettings,
    schedule_receiver: mpsc::UnboundedReceiver<Vec<u8>>,
    emission_sender: mpsc::UnboundedSender<Vec<u8>>,
) {
    let mut schedule_receiver = schedule_receiver;
    let mut interval = time::interval(Duration::from_secs_f64(
        1.0 / settings.max_emission_frequency,
    ));
    let mut coin = Coin::<_>::new(
        ChaCha12Rng::from_entropy(),
        settings.drop_message_probability,
    )
    .unwrap();

    loop {
        interval.tick().await;

        // Emit the first one of the scheduled messages.
        // If there is no scheduled message, emit a drop message with probability.
        match schedule_receiver.try_recv() {
            Ok(msg) => {
                if let Err(e) = emission_sender.send(msg) {
                    tracing::error!("Failed to send message to the transmission channel: {e:?}");
                }
            }
            Err(TryRecvError::Empty) => {
                // If the coin is head, emit the drop message.
                if coin.flip() {
                    if let Err(e) = emission_sender.send(DROP_MESSAGE.to_vec()) {
                        tracing::error!(
                            "Failed to send drop message to the transmission channel: {e:?}"
                        );
                    }
                }
            }
            Err(TryRecvError::Disconnected) => {
                tracing::error!("The schedule channel has been closed");
                break;
            }
        }
    }
}

struct Coin<R: Rng> {
    rng: R,
    distribution: Uniform<f64>,
    probability: f64,
}

impl<R: Rng> Coin<R> {
    fn new(rng: R, probability: f64) -> Result<Self, CoinError> {
        if !(0.0..=1.0).contains(&probability) {
            return Err(CoinError::InvalidProbability);
        }
        Ok(Self {
            rng,
            distribution: Uniform::from(0.0..1.0),
            probability,
        })
    }

    // Flip the coin based on the given probability.
    fn flip(&mut self) -> bool {
        self.distribution.sample(&mut self.rng) < self.probability
    }
}

#[derive(Debug)]
enum CoinError {
    InvalidProbability,
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::StreamExt;

    macro_rules! assert_interval {
        ($last_time:expr, $lower_bound:expr, $upper_bound:expr) => {
            let now = time::Instant::now();
            let interval = now.duration_since(*$last_time);

            assert!(
                interval >= $lower_bound,
                "interval {:?} is too short. lower_bound: {:?}",
                interval,
                $lower_bound,
            );
            assert!(
                interval <= $upper_bound,
                "interval {:?} is too long. upper_bound: {:?}",
                interval,
                $upper_bound,
            );

            *$last_time = now;
        };
    }

    #[tokio::test]
    async fn test_persistent_transmission() {
        let (schedule_sender, schedule_receiver) = mpsc::unbounded_channel();
        let (emission_sender, mut emission_receiver) = mpsc::unbounded_channel();

        let settings = PersistentTransmissionSettings {
            max_emission_frequency: 1.0,
            // Set to always emit drop messages if no scheduled messages for easy testing
            drop_message_probability: 1.0,
        };

        // Prepare the expected emission interval with torelance
        let expected_emission_interval =
            Duration::from_secs_f64(1.0 / settings.max_emission_frequency);
        let torelance = expected_emission_interval / 10; // 10% torelance
        let lower_bound = expected_emission_interval - torelance;
        let upper_bound = expected_emission_interval + torelance;

        // Start the persistent transmission and schedule messages
        tokio::spawn(persistent_transmission(
            settings,
            schedule_receiver,
            emission_sender,
        ));
        // Messages must be scheduled in non-blocking manner.
        schedule_sender.send(vec![1]).unwrap();
        schedule_sender.send(vec![2]).unwrap();
        schedule_sender.send(vec![3]).unwrap();

        // Check if expected messages are emitted with the expected interval
        assert_eq!(emission_receiver.recv().await.unwrap(), vec![1]);
        let mut last_time = time::Instant::now();

        assert_eq!(emission_receiver.recv().await.unwrap(), vec![2]);
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(emission_receiver.recv().await.unwrap(), vec![3]);
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(
            emission_receiver.recv().await.unwrap(),
            DROP_MESSAGE.to_vec()
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(
            emission_receiver.recv().await.unwrap(),
            DROP_MESSAGE.to_vec()
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        // Schedule a new message and check if it is emitted at the next interval
        schedule_sender.send(vec![4]).unwrap();
        assert_eq!(emission_receiver.recv().await.unwrap(), vec![4]);
        assert_interval!(&mut last_time, lower_bound, upper_bound);
    }

    #[tokio::test]
    async fn test_persistent_transmission_stream() {
        let (schedule_sender, schedule_receiver) = mpsc::unbounded_channel();
        let stream = tokio_stream::wrappers::UnboundedReceiverStream::new(schedule_receiver);
        let settings = PersistentTransmissionSettings {
            max_emission_frequency: 1.0,
            // Set to always emit drop messages if no scheduled messages for easy testing
            drop_message_probability: 1.0,
        };
        // Prepare the expected emission interval with torelance
        let expected_emission_interval =
            Duration::from_secs_f64(1.0 / settings.max_emission_frequency);
        let torelance = expected_emission_interval / 10; // 10% torelance
        let lower_bound = expected_emission_interval - torelance;
        let upper_bound = expected_emission_interval + torelance;
        // prepare stream
        let mut persistent_transmission_stream = stream.persistent_transmission(settings).boxed();
        // Messages must be scheduled in non-blocking manner.
        schedule_sender.send(vec![1]).unwrap();
        schedule_sender.send(vec![2]).unwrap();
        schedule_sender.send(vec![3]).unwrap();

        // Check if expected messages are emitted with the expected interval
        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            vec![1]
        );
        let mut last_time = time::Instant::now();

        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            vec![2]
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            vec![3]
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            DROP_MESSAGE.to_vec()
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            DROP_MESSAGE.to_vec()
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);

        // Schedule a new message and check if it is emitted at the next interval
        schedule_sender.send(vec![4]).unwrap();
        assert_eq!(
            persistent_transmission_stream.next().await.unwrap(),
            vec![4]
        );
        assert_interval!(&mut last_time, lower_bound, upper_bound);
    }
}
