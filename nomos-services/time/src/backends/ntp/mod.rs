mod async_client;

use crate::backends::common::slot_timer;
use crate::backends::ntp::async_client::{AsyncNTPClient, NTPClientSettings};
use crate::backends::TimeBackend;
use crate::{EpochSlotTickStream, SlotTick};
use cryptarchia_engine::{EpochConfig, Slot, SlotConfig};
use futures::{Stream, StreamExt};
use sntpc::{fraction_to_nanoseconds, NtpResult};
use std::pin::Pin;
use std::task::{Context, Poll};
use std::time::Duration;
use time::OffsetDateTime;
use tokio::time::{interval, MissedTickBehavior};
use tokio_stream::wrappers::IntervalStream;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
pub struct NtpSettings {
    pub ntp_server: String,
    pub timeout: Duration,
    pub update_interval: Duration,
    pub connect_timeout: Duration,
    pub slot_config: SlotConfig,
    pub epoch_config: EpochConfig,
    pub base_period_length: u64,
    pub ntpclient_settings: NTPClientSettings,
}
pub struct Ntp {
    settings: NtpSettings,
    client: AsyncNTPClient,
}

impl TimeBackend for Ntp {
    type Settings = NtpSettings;

    fn init(settings: Self::Settings) -> Self {
        let client = AsyncNTPClient::new(settings.ntpclient_settings);
        Self { settings, client }
    }

    fn tick_stream(self) -> EpochSlotTickStream {
        let Self { settings, client } = self;
        let mut update_interval = interval(settings.update_interval);
        update_interval.set_missed_tick_behavior(MissedTickBehavior::Skip);
        let ntp_server = settings.ntp_server.clone();
        let interval: NtpResultStream = Pin::new(Box::new(
            IntervalStream::new(update_interval)
                .zip(futures::stream::repeat((client, ntp_server)))
                .filter_map(move |(_, (client, ntp_server))| {
                    Box::pin(async move { client.request_timestamp(ntp_server).await.ok() })
                }),
        ));
        let local_date = OffsetDateTime::now_utc();
        let slot_timer = slot_timer(
            settings.slot_config,
            local_date,
            Slot::current_from_offset_and_config(local_date, settings.slot_config),
            settings.epoch_config,
            settings.base_period_length,
        );
        Pin::new(Box::new(NtpStream {
            interval,
            slot_config: settings.slot_config,
            epoch_config: settings.epoch_config,
            base_period_length: settings.base_period_length,
            slot_timer,
        }))
    }
}

type NtpResultStream = Pin<Box<dyn Stream<Item = NtpResult> + Send + Sync + Unpin>>;

pub struct NtpStream {
    interval: NtpResultStream,
    slot_config: SlotConfig,
    epoch_config: EpochConfig,
    base_period_length: u64,
    slot_timer: EpochSlotTickStream,
}
impl Stream for NtpStream {
    type Item = SlotTick;

    fn poll_next(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Option<Self::Item>> {
        if let Poll::Ready(Some(timestamp)) = self.interval.poll_next_unpin(cx) {
            let seconds = Duration::from_secs(timestamp.sec() as u64);
            let nanos_fraction =
                Duration::from_nanos(fraction_to_nanoseconds(timestamp.sec_fraction()) as u64);
            let roundtrip = Duration::from_micros(timestamp.roundtrip());

            let date = OffsetDateTime::from_unix_timestamp_nanos(
                (seconds + nanos_fraction + roundtrip).as_nanos() as i128,
            )
            .expect("Datetime synchronization failed");
            let current_slot = Slot::current_from_offset_and_config(date, self.slot_config);
            let epoch_config = self.epoch_config;
            let base_period_length = self.base_period_length;
            self.slot_timer = slot_timer(
                self.slot_config,
                date,
                current_slot,
                epoch_config,
                base_period_length,
            );
        }
        self.slot_timer.poll_next_unpin(cx)
    }
}
