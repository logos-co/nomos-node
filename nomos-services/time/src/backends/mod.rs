mod common;
#[cfg(feature = "ntp")]
pub mod ntp;
pub mod system_time;

use crate::EpochSlotTickStream;

/// Abstraction over slot ticking systems
pub trait TimeBackend {
    type Settings;
    fn init(settings: Self::Settings) -> Self;
    fn tick_stream(self) -> EpochSlotTickStream;
}
