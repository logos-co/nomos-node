use std::str::FromStr;
use std::time::Duration;
use time::OffsetDateTime;

const DEFAULT_SLOT_TIME: u64 = 2;
const CONSENSUS_SLOT_TIME_VAR: &str = "CONSENSUS_SLOT_TIME";

#[derive(Clone, Debug)]
pub struct TimeConfig {
    slot_duration: Duration,
    chain_start_time: OffsetDateTime,
}

pub fn default_time_config() -> TimeConfig {
    let slot_duration = std::env::var(CONSENSUS_SLOT_TIME_VAR)
        .map(|s| <u64>::from_str(&s).unwrap())
        .unwrap_or(DEFAULT_SLOT_TIME);
    TimeConfig {
        slot_duration: Duration::from_secs(slot_duration),
        chain_start_time: OffsetDateTime::now_utc(),
    }
}
