use cryptarchia_engine::{Epoch, Slot};

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq)]
pub struct Config {
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
    pub consensus_config: cryptarchia_engine::Config,
}

impl Config {
    pub fn base_period_length(&self) -> u64 {
        self.consensus_config.base_period_length()
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
        let base = (u32::from(epoch) - 1) as u64 * self.epoch_length();
        (base + offset).into()
    }

    pub fn stake_distribution_snapshot(&self, epoch: Epoch) -> Slot {
        ((u32::from(epoch) - 1) as u64 * self.epoch_length()).into()
    }

    pub fn epoch(&self, slot: Slot) -> Epoch {
        ((u64::from(slot) / self.epoch_length()) as u32).into()
    }
}

#[cfg(test)]
mod tests {
    #[test]
    fn epoch_snapshots() {
        let config = super::Config {
            epoch_stake_distribution_stabilization: 3,
            epoch_period_nonce_buffer: 3,
            epoch_period_nonce_stabilization: 4,
            consensus_config: cryptarchia_engine::Config {
                security_param: 5,
                active_slot_coeff: 0.5,
            },
        };
        assert_eq!(config.epoch_length(), 100);
        assert_eq!(config.nonce_snapshot(1.into()), 60.into());
        assert_eq!(config.nonce_snapshot(2.into()), 160.into());
        assert_eq!(config.stake_distribution_snapshot(1.into()), 0.into());
        assert_eq!(config.stake_distribution_snapshot(2.into()), 100.into());
    }

    #[test]
    fn slot_to_epoch() {
        let config = super::Config {
            epoch_stake_distribution_stabilization: 3,
            epoch_period_nonce_buffer: 3,
            epoch_period_nonce_stabilization: 4,
            consensus_config: cryptarchia_engine::Config {
                security_param: 5,
                active_slot_coeff: 0.5,
            },
        };
        assert_eq!(config.epoch(1.into()), 0.into());
        assert_eq!(config.epoch(100.into()), 1.into());
        assert_eq!(config.epoch(101.into()), 1.into());
        assert_eq!(config.epoch(200.into()), 2.into());
    }
}
