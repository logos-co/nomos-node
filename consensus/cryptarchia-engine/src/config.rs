use crate::{Epoch, Slot};

#[derive(Clone, Debug, PartialEq)]
pub struct TimeConfig {
    // How long a slot lasts in seconds
    pub slot_duration: u64,
    // Start of the first epoch, in unix timestamp second precision
    pub chain_start_time: u64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Config {
    // Depth of forks that trigger the density rule
    pub k: u8,
    // Number of slots after forking point to consider for the density rule
    pub s: u8,
    // f, the rate of occupied slots
    pub active_slot_coeff: f64,
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
    pub time: TimeConfig,
}
impl Config {
    pub fn time_config(&self) -> &TimeConfig {
        &self.time
    }

    pub fn base_period_length(&self) -> u64 {
        (f64::from(self.k) / self.active_slot_coeff).floor() as u64
    }

    pub fn epoch_length(&self) -> u64 {
        (self.epoch_stake_distribution_stabilization as u64
            + self.epoch_period_nonce_buffer as u64
            + self.epoch_period_nonce_stabilization as u64)
            * self.base_period_length()
    }

    pub fn nonce_snapshot(&self, epoch: Epoch) -> Slot {
        let offset = self.base_period_length()
            * (self.epoch_period_nonce_buffer + self.epoch_stake_distribution_stabilization) as u64;
        let base = u32::from(epoch) as u64 * self.epoch_length();
        (base + offset).into()
    }

    pub fn stake_distribution_snapshot(&self, epoch: Epoch) -> Slot {
        (u32::from(epoch) as u64 * self.epoch_length()).into()
    }
}
