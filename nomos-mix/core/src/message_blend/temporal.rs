use std::{
    collections::VecDeque,
    pin::Pin,
    task::{Context, Poll},
    time::Duration,
};

use futures::{Future, Stream, StreamExt};
use rand::{Rng, RngCore};
use serde::{Deserialize, Serialize};
use tokio::time;

/// [`TemporalProcessor`] delays messages randomly to hide timing correlation
/// between incoming and outgoing messages from a node.
///
/// See the [`Stream`] implementation below for more details on how it works.
pub(crate) struct TemporalProcessor<M, Rng> {
    settings: TemporalProcessorSettings,
    // All scheduled messages
    queue: VecDeque<M>,
    /// Interval in seconds for running the lottery to release a message
    lottery_interval: time::Interval,
    /// To wait a few seconds after running the lottery before releasing the message.
    /// The lottery returns how long to wait before releasing the message.
    release_timer: Option<Pin<Box<time::Sleep>>>,
    /// local lottery rng
    rng: Rng,
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize)]
pub struct TemporalProcessorSettings {
    pub max_delay_seconds: u64,
}

impl<M, Rng> TemporalProcessor<M, Rng> {
    pub(crate) fn new(settings: TemporalProcessorSettings, rng: Rng) -> Self {
        let lottery_interval = Self::lottery_interval(settings.max_delay_seconds);
        Self {
            settings,
            queue: VecDeque::new(),
            lottery_interval,
            release_timer: None,
            rng,
        }
    }

    /// Create [`time::Interval`] for running the lottery to release a message.
    fn lottery_interval(max_delay_seconds: u64) -> time::Interval {
        time::interval(Duration::from_secs(Self::lottery_interval_seconds(
            max_delay_seconds,
        )))
    }

    /// Calculate the interval in seconds for running the lottery.
    /// The lottery interval is half of the maximum delay,
    /// in order to guarantee that the interval between two subsequent message emissions
    /// is at most [`max_delay_seconds`].
    fn lottery_interval_seconds(max_delay_seconds: u64) -> u64 {
        max_delay_seconds / 2
    }
    /// Schedule a message to be released later.
    pub(crate) fn push_message(&mut self, message: M) {
        self.queue.push_back(message);
    }
}
impl<M, Rng> TemporalProcessor<M, Rng>
where
    Rng: RngCore,
{
    /// Run the lottery to determine the delay before releasing a message.
    /// The delay is in [0, `lottery_interval_seconds`).
    fn run_lottery(&mut self) -> u64 {
        let interval = Self::lottery_interval_seconds(self.settings.max_delay_seconds);
        self.rng.gen_range(0..interval)
    }
}

impl<M, Rng> Stream for TemporalProcessor<M, Rng>
where
    M: Unpin,
    Rng: RngCore + Unpin,
{
    type Item = M;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        // Check whether it's time to run a new lottery to determine the delay.
        if self.lottery_interval.poll_tick(cx).is_ready() {
            let delay = self.run_lottery();
            // Set timer to release the message after the delay.
            self.release_timer = Some(Box::pin(time::sleep(Duration::from_secs(delay))));
        }

        // Check whether the release timer is done if it exists.
        if let Some(timer) = self.release_timer.as_mut() {
            if timer.as_mut().poll(cx).is_ready() {
                self.release_timer.take(); // Reset timer after it's done
                if let Some(msg) = self.queue.pop_front() {
                    // Release the 1st message in the queue if it exists.
                    return Poll::Ready(Some(msg));
                }
            }
        }

        Poll::Pending
    }
}

pub struct TemporalStream<S, Rng>
where
    S: Stream,
    Rng: RngCore,
{
    processor: TemporalProcessor<S::Item, Rng>,
    wrapped_stream: S,
}

impl<S, Rng> Stream for TemporalStream<S, Rng>
where
    S: Stream + Unpin,
    S::Item: Unpin,
    Rng: RngCore + Unpin,
{
    type Item = S::Item;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(item)) = self.wrapped_stream.poll_next_unpin(cx) {
            self.processor.push_message(item);
        }
        self.processor.poll_next_unpin(cx)
    }
}
pub trait TemporalProcessorExt<Rng>: Stream
where
    Rng: RngCore,
{
    fn temporal_stream(
        self,
        settings: TemporalProcessorSettings,
        rng: Rng,
    ) -> TemporalStream<Self, Rng>
    where
        Self: Sized,
    {
        TemporalStream {
            processor: TemporalProcessor::new(settings, rng),
            wrapped_stream: self,
        }
    }
}

impl<T, Rng> TemporalProcessorExt<Rng> for T
where
    T: Stream,
    Rng: RngCore,
{
}
