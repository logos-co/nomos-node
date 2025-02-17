mod common;
#[cfg(feature = "ntp")]
pub mod ntp;
mod system_time;

use crate::EpochSlotTickStream;
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub trait TimeBackend {
    type Settings;
    fn init(settings: Self::Settings) -> Self;
    fn tick_stream(self) -> EpochSlotTickStream;
}
