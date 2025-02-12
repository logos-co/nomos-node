use crate::backends::TimeBackend;
use crate::SlotTickStream;
use overwatch_rs::overwatch::handle::OverwatchHandle;

pub struct Ntp {}

impl TimeBackend for Ntp {
    type Settings = ();

    fn init(settings: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        todo!()
    }

    fn tick_stream(&self) -> SlotTickStream {
        todo!()
    }
}
