use crate::config::Config;
use std::ops::Add;

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct Slot(u64);

#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash, PartialOrd, Ord)]
pub struct Epoch(u32);

impl Slot {
    pub fn to_be_bytes(&self) -> [u8; 8] {
        self.0.to_be_bytes()
    }

    pub fn genesis() -> Self {
        Self(0)
    }

    pub fn epoch(&self, config: &Config) -> Epoch {
        Epoch((self.0 / config.epoch_length()) as u32)
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
