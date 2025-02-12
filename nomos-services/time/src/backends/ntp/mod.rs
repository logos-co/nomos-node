mod async_client;

use crate::backends::ntp::async_client::AsyncNTPClient;
use crate::backends::TimeBackend;
use crate::SlotTickStream;
use overwatch_rs::overwatch::handle::OverwatchHandle;
use std::time::Duration;
use url::Url;

pub struct NtpSettings {
    server_urls: Vec<Url>,
    timeout: Duration,
    connect_timeout: Duration,
}
pub struct Ntp {
    settings: NtpSettings,
    client: AsyncNTPClient,
}

impl TimeBackend for Ntp {
    type Settings = NtpSettings;

    fn init(settings: Self::Settings, overwatch_handle: OverwatchHandle) -> Self {
        Self { settings, client }
    }

    fn tick_stream(&self) -> SlotTickStream {
        todo!()
    }
}
