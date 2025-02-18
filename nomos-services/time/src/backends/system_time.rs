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
