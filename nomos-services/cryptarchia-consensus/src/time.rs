use cryptarchia_engine::Slot;
use serde::{Deserialize, Serialize};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::time::{Interval, MissedTickBehavior};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Config {
    pub slot_duration: Duration,
    /// Start of the first epoch
    pub chain_start_time: OffsetDateTime,
}

#[derive(Clone, Debug)]
pub struct Timer {
    config: Config,
}

impl Timer {
    pub fn new(config: Config) -> Self {
        Timer { config }
    }

    pub fn current_slot(&self) -> Slot {
        // TODO: leap seconds / weird time stuff
        let since_start = OffsetDateTime::now_utc() - self.config.chain_start_time;
        if since_start.is_negative() {
            tracing::warn!("Current slot is before the start of the chain");
            Slot::genesis()
        } else {
            Slot::from(since_start.whole_seconds() as u64 / self.config.slot_duration.as_secs())
        }
    }

    /// Ticks at the start of each slot, starting from the next slot
    pub fn slot_interval(&self) -> Interval {
        let slot_duration = self.config.slot_duration;
        let now = OffsetDateTime::now_utc();
        let next_slot_start = self.config.chain_start_time
            + slot_duration * u64::from(self.current_slot() + 1) as u32;
        let delay = next_slot_start - now;
        let mut interval = tokio::time::interval_at(
            tokio::time::Instant::now()
                + Duration::try_from(delay).expect("could not set slot timer duration"),
            slot_duration,
        );
        interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        interval
    }
}
