use std::{ops::Range, time::Duration};

use mixnet_client::MixnetClientConfig;
use nomos_libp2p::{Multiaddr, SwarmConfig};
use serde::{Deserialize, Serialize};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Libp2pConfig {
    #[serde(flatten)]
    pub inner: SwarmConfig,
    // Initial peers to connect to
    #[serde(default)]
    pub initial_peers: Vec<Multiaddr>,
    pub mixnet_client: MixnetClientConfig,
    #[serde(with = "humantime")]
    pub mixnet_delay: Range<Duration>,
}

mod humantime {
    use serde::{Deserialize, Deserializer, Serialize, Serializer};
    use std::{ops::Range, time::Duration};

    #[derive(Serialize, Deserialize)]
    struct DurationRangeHelper {
        #[serde(with = "humantime_serde")]
        start: Duration,
        #[serde(with = "humantime_serde")]
        end: Duration,
    }

    pub fn serialize<S: Serializer>(
        val: &Range<Duration>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            DurationRangeHelper {
                start: val.start,
                end: val.end,
            }
            .serialize(serializer)
        } else {
            val.serialize(serializer)
        }
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<Range<Duration>, D::Error> {
        if deserializer.is_human_readable() {
            let DurationRangeHelper { start, end } =
                DurationRangeHelper::deserialize(deserializer)?;
            Ok(start..end)
        } else {
            Range::<Duration>::deserialize(deserializer)
        }
    }
}
