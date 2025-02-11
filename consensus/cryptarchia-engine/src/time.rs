use std::{num::NonZero, ops::Add, time::Duration};

use time::OffsetDateTime;
#[cfg(feature = "tokio")]
use tokio::time::{Interval, MissedTickBehavior};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct Slot(u64);

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct Epoch(u32);

impl Slot {
    pub fn to_be_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    pub fn genesis() -> Self {
        Self(0)
    }

    pub fn from_offset_and_config(
        offset_date_time: OffsetDateTime,
        slot_config: SlotConfig,
    ) -> Self {
        // TODO: leap seconds / weird time stuff
        let since_start = offset_date_time - slot_config.chain_start_time;
        if since_start.is_negative() {
            // current slot is behind the start time, so return default 0
            Slot::genesis()
        } else {
            // safety: since_start is already checked never negative in this case
            // division panics if `slot_duration` is less than a second.
            Slot::from(
                (since_start.whole_seconds() as u64)
                    .checked_div(slot_config.slot_duration.as_secs())
                    .expect("slots tick should be at least a second"),
            )
        }
    }
}

impl From<u32> for Epoch {
    fn from(epoch: u32) -> Self {
        Self(epoch)
    }
}

impl From<Epoch> for u32 {
    fn from(epoch: Epoch) -> Self {
        epoch.0
    }
}

impl TryFrom<u64> for Epoch {
    type Error = <u64 as TryInto<u32>>::Error;

    fn try_from(value: u64) -> Result<Self, Self::Error> {
        value.try_into().map(Self)
    }
}

impl From<u64> for Slot {
    fn from(slot: u64) -> Self {
        Self(slot)
    }
}

impl From<Slot> for u64 {
    fn from(slot: Slot) -> Self {
        slot.0
    }
}

impl Add<u64> for Slot {
    type Output = Slot;

    fn add(self, rhs: u64) -> Self::Output {
        Slot(self.0 + rhs)
    }
}

impl Add<u32> for Epoch {
    type Output = Epoch;

    fn add(self, rhs: u32) -> Self::Output {
        Epoch(self.0 + rhs)
    }
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct EpochConfig {
    // The stake distribution is always taken at the beginning of the previous epoch.
    // This parameters controls how many slots to wait for it to be stabilized
    // The value is computed as epoch_stake_distribution_stabilization * int(floor(k / f))
    pub epoch_stake_distribution_stabilization: u8,
    // This parameter controls how many slots we wait after the stake distribution
    // snapshot has stabilized to take the nonce snapshot.
    pub epoch_period_nonce_buffer: u8,
    // This parameter controls how many slots we wait for the nonce snapshot to be considered
    // stabilized
    pub epoch_period_nonce_stabilization: u8,
}

impl EpochConfig {
    pub fn epoch_length(&self, base_period_length: u64) -> u64 {
        (self.epoch_stake_distribution_stabilization as u64
            + self.epoch_period_nonce_buffer as u64
            + self.epoch_period_nonce_stabilization as u64)
            * base_period_length
    }

    pub fn epoch(&self, slot: Slot, base_period_length: u64) -> Epoch {
        ((u64::from(slot) / base_period_length) as u32).into()
    }
}
