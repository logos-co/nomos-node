mod ntp;

use crate::SlotTickStream;
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub trait TimeBackend {
    type Settings;
    fn init(settings: Self::Settings, overwatch_handle: OverwatchHandle) -> Self;
    fn tick_stream(&self) -> SlotTickStream;
}
