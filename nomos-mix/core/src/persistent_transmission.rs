use futures::Stream;
use nomos_mix_message::MixMessage;
use rand::{distributions::Uniform, prelude::Distribution, Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use std::pin::{pin, Pin};
use std::task::{Context, Poll};
use std::time::Duration;
use tokio::time;
use tokio::time::Interval;

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
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

/// Transmit scheduled messages with a persistent rate as a stream.
pub struct PersistentTransmissionStream<S, M>
where
    S: Stream,
{
    interval: Interval,
    coin: Coin<ChaCha12Rng>,
    stream: S,
    _mix_message: PhantomData<M>,
}

impl<S, M> PersistentTransmissionStream<S, M>
where
    S: Stream,
    M: MixMessage,
{
    pub fn new(
        settings: PersistentTransmissionSettings,
        stream: S,
    ) -> PersistentTransmissionStream<S, M> {
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
            stream,
            _mix_message: Default::default(),
        }
    }
}

impl<S, M> Stream for PersistentTransmissionStream<S, M>
where
    S: Stream<Item = Vec<u8>> + Unpin,
    M: MixMessage + Unpin,
{
    type Item = Vec<u8>;

    fn poll_next(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        let Self {
            ref mut interval,
            ref mut stream,
            ref mut coin,
            ..
        } = self.get_mut();
        if pin!(interval).poll_tick(cx).is_pending() {
            return Poll::Pending;
        }
        if let Poll::Ready(Some(item)) = pin!(stream).poll_next(cx) {
            Poll::Ready(Some(item))
        } else if coin.flip() {
            Poll::Ready(Some(M::DROP_MESSAGE.to_vec()))
        } else {
            Poll::Pending
        }
    }
}

pub trait PersistentTransmissionExt<M>: Stream
where
    M: MixMessage,
{
    fn persistent_transmission(
        self,
        settings: PersistentTransmissionSettings,
    ) -> PersistentTransmissionStream<Self, M>
    where
        Self: Sized + Unpin,
    {
        PersistentTransmissionStream::new(settings, self)
    }
}

impl<S, M> PersistentTransmissionExt<M> for S
where
    S: Stream,
    M: MixMessage,
    M::PublicKey: Clone + Serialize + DeserializeOwned,
{
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
    use tokio::sync::mpsc;

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
        let mut persistent_transmission_stream = stream.persistent_transmission(settings);
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
