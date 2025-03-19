use std::time::Duration;

use serde::{Deserialize, Serialize};

pub mod balancer;
pub mod handlers;
pub mod monitor;
pub mod policy;

#[derive(Copy, Clone, Debug, Default, Serialize, Deserialize)]
pub struct DAReplicationSettings {
    pub seen_message_cache_size: usize,
    pub seen_message_ttl: Duration,
}
