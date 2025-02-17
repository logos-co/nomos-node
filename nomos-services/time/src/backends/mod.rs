mod common;
#[cfg(feature = "ntp")]
pub mod ntp;
mod system_time;

use crate::EpochSlotTickStream;

pub trait TimeBackend {
    type Settings;
    fn init(settings: Self::Settings) -> Self;
    fn tick_stream(self) -> EpochSlotTickStream;
}
