use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use mixnet_protocol::connection::ConnectionPoolConfig;
use nym_sphinx::{PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::{PRIVATE_KEY_SIZE, PUBLIC_KEY_SIZE};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetNodeConfig {
    /// A listen address for receiving Sphinx packets
    pub listen_address: SocketAddr,
    /// An listen address for communicating with mixnet clients
    pub client_listen_address: SocketAddr,
    /// A key for decrypting Sphinx packets
    pub private_key: [u8; PRIVATE_KEY_SIZE],
    /// The maximum number of retries.
    #[serde(default = "MixnetNodeConfig::default_max_net_write_tries")]
    pub max_net_write_tries: usize,
    /// Connection pool config
    #[serde(default = "ConnectionPoolConfig::default")]
    pub connection_pool_config: ConnectionPoolConfig,
}

impl Default for MixnetNodeConfig {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7777)),
            client_listen_address: SocketAddr::V4(SocketAddrV4::new(
                Ipv4Addr::new(127, 0, 0, 1),
                7778,
            )),
            private_key: PrivateKey::new().to_bytes(),
            max_net_write_tries: MixnetNodeConfig::default_max_net_write_tries(),
            connection_pool_config: ConnectionPoolConfig::default(),
        }
    }
}

impl MixnetNodeConfig {
    const fn default_max_net_write_tries() -> usize {
        3
    }

    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *PublicKey::from(&PrivateKey::from(self.private_key)).as_bytes()
    }
}
