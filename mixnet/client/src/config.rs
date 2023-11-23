use std::net::ToSocketAddrs;

use futures::{stream, StreamExt};
use mixnet_protocol::connection::ConnectionPoolConfig;
use mixnet_topology::MixnetTopology;
use serde::{Deserialize, Serialize};

use crate::{receiver::Receiver, MessageStream, MixnetClientError};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetClientConfig {
    pub mode: MixnetClientMode,
    pub topology: MixnetTopology,
    #[serde(default = "MixnetClientConfig::default_max_net_write_tries")]
    pub max_net_write_tries: usize,
    #[serde(default = "ConnectionPoolConfig::default")]
    pub connection_pool_config: ConnectionPoolConfig,
}

impl MixnetClientConfig {
    /// Creates a new `MixnetClientConfig` with default values.
    pub fn new(mode: MixnetClientMode, topology: MixnetTopology) -> Self {
        Self {
            mode,
            topology,
            max_net_write_tries: Self::default_max_net_write_tries(),
            connection_pool_config: ConnectionPoolConfig::default(),
        }
    }

    const fn default_max_net_write_tries() -> usize {
        3
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub enum MixnetClientMode {
    Sender,
    SenderReceiver(String),
}

impl MixnetClientMode {
    pub(crate) async fn run(&self) -> Result<MessageStream, MixnetClientError> {
        match self {
            Self::Sender => Ok(stream::empty().boxed()),
            Self::SenderReceiver(node_address) => {
                let mut addrs = node_address
                    .to_socket_addrs()
                    .map_err(|e| MixnetClientError::MixnetNodeAddressError(e.to_string()))?;
                let socket_addr = addrs
                    .next()
                    .ok_or(MixnetClientError::MixnetNodeAddressError(
                        "No address provided".into(),
                    ))?;
                Ok(Receiver::new(socket_addr).run().await?.boxed())
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
        assert_eq!(conf.mode, MixnetClientMode::Sender);
        assert_eq!(conf.topology, MixnetTopology { layers: Vec::new() });
        assert_eq!(
            conf.max_net_write_tries,
            MixnetClientConfig::default_max_net_write_tries()
        );
    }
}
