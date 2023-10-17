use std::{net::SocketAddr, time::Duration};

use futures::{stream, StreamExt};
use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

use crate::{receiver::Receiver, MessageStream, MixnetClientError};

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
    #[serde(default = "MixnetClientConfig::default_connection_pool_size")]
    pub connection_pool_size: usize,
    #[serde(default = "MixnetClientConfig::default_max_retries")]
    pub max_retries: usize,
    #[serde(default = "MixnetClientConfig::default_retry_delay")]
    pub retry_delay: std::time::Duration,
}

impl MixnetClientConfig {
    /// Creates a new `MixnetClientConfig` with default values.
    pub fn new(mode: MixnetClientMode, topology: MixnetTopology) -> Self {
        Self {
            mode,
            topology,
            connection_pool_size: Self::default_connection_pool_size(),
            max_retries: Self::default_max_retries(),
            retry_delay: Self::default_retry_delay(),
        }
    }

    const fn default_connection_pool_size() -> usize {
        256
    }

    const fn default_max_retries() -> usize {
        3
    }

    const fn default_retry_delay() -> Duration {
        Duration::from_secs(5)
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, Eq, PartialEq)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(SocketAddr),
}

impl MixnetClientMode {
    pub(crate) async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        match self {
            Self::Sender => Ok(stream::empty().boxed()),
            Self::SenderReceiver(node_address) => {
                Ok(Receiver::new(*node_address).run().await?.boxed())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use mixnet_topology::MixnetTopology;

    use crate::{MixnetClientConfig, MixnetClientMode};

    #[test]
    fn default_config_serde() {
        let yaml = "
            mode: Sender
            topology:
              layers: []
        ";
        let conf: MixnetClientConfig = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(
            conf,
            MixnetClientConfig::new(
                MixnetClientMode::Sender,
                MixnetTopology { layers: Vec::new() }
            )
        );
    }
}
