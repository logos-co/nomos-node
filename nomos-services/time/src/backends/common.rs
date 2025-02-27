use crate::{EpochSlotTickStream, SlotTick};
use cryptarchia_engine::time::SlotTimer;
use cryptarchia_engine::{EpochConfig, Slot, SlotConfig};
use futures::StreamExt;
use std::num::NonZero;
use std::pin::Pin;
use time::OffsetDateTime;
use tokio_stream::wrappers::IntervalStream;

pub(crate) fn slot_timer(
    slot_config: SlotConfig,
    datetime: OffsetDateTime,
    current_slot: Slot,
    epoch_config: EpochConfig,
    base_period_length: NonZero<u64>,
) -> EpochSlotTickStream {
    Pin::new(Box::new(
        IntervalStream::new(SlotTimer::new(slot_config).slot_interval(datetime))
            .zip(futures::stream::iter(std::iter::successors(
                Some(current_slot),
                |&slot| Some(slot + 1),
            )))
            .map(move |(_, slot)| SlotTick {
                epoch: epoch_config.epoch(slot, base_period_length),
                slot,
            }),
    ))
}
