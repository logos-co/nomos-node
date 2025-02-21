use crate::backends::common::slot_timer;
use crate::backends::TimeBackend;
use crate::EpochSlotTickStream;
use cryptarchia_engine::{EpochConfig, Slot, SlotConfig};
use time::OffsetDateTime;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug)]
pub struct SystemTimeBackendSettings {
    pub slot_config: SlotConfig,
    pub epoch_config: EpochConfig,
    pub base_period_length: u64,
}

pub struct SystemTimeBackend {
    settings: SystemTimeBackendSettings,
}

impl TimeBackend for SystemTimeBackend {
    type Settings = SystemTimeBackendSettings;

    fn init(settings: Self::Settings) -> Self {
        Self { settings }
    }

    fn tick_stream(self) -> EpochSlotTickStream {
        let Self { settings } = self;
        let local_date = OffsetDateTime::now_utc();
        slot_timer(
            settings.slot_config,
            local_date,
            Slot::current_from_offset_and_config(local_date, settings.slot_config),
            settings.epoch_config,
            settings.base_period_length,
        )
    }
}

#[cfg(test)]
mod test {
    use crate::backends::system_time::{SystemTimeBackend, SystemTimeBackendSettings};
    use crate::backends::TimeBackend;
    use cryptarchia_engine::{EpochConfig, Slot, SlotConfig};
    use futures::StreamExt;
    use std::time::Duration;
    use time::OffsetDateTime;

    #[tokio::test]
    async fn test_stream() {
        const SAMPLE_SIZE: u64 = 5;
        let expected: Vec<_> = (0..SAMPLE_SIZE).map(Slot::from).collect();
        let settings = SystemTimeBackendSettings {
            slot_config: SlotConfig {
                slot_duration: Duration::from_secs(1),
                chain_start_time: OffsetDateTime::now_utc(),
            },
            epoch_config: EpochConfig {
                epoch_stake_distribution_stabilization: 0,
                epoch_period_nonce_buffer: 0,
                epoch_period_nonce_stabilization: 0,
            },
            base_period_length: 10,
        };
        let backend = SystemTimeBackend::init(settings);
        let stream = backend.tick_stream();
        let result: Vec<_> = stream
            .take(SAMPLE_SIZE as usize)
            .map(|slot_tick| slot_tick.slot)
            .collect()
            .await;
        assert_eq!(expected, result);
    }
}
