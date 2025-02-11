use std::ops::Add;

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
