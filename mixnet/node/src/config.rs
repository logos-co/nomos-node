use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use nym_sphinx::{PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::{PRIVATE_KEY_SIZE, PUBLIC_KEY_SIZE};

#[derive(Serialize, Deserialize, Copy, Clone, Debug)]
pub struct MixnetNodeConfig {
    /// A listen address for receiving Sphinx packets
    pub listen_address: SocketAddr,
    /// An listen address fro communicating with mixnet clients
    pub client_listen_address: SocketAddr,
    /// A key for decrypting Sphinx packets
    pub private_key: [u8; PRIVATE_KEY_SIZE],
    /// The size of the connection pool.
    #[serde(default = "MixnetNodeConfig::default_connection_pool_size")]
    pub connection_pool_size: usize,
    /// The maximum number of retries.
    #[serde(default = "MixnetNodeConfig::default_max_retries")]
    pub max_retries: usize,
    /// The retry delay between retries.
    #[serde(default = "MixnetNodeConfig::default_retry_delay")]
    pub retry_delay: Duration,
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
            connection_pool_size: 255,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
        }
    }
}

impl MixnetNodeConfig {
    const fn default_connection_pool_size() -> usize {
        255
    }

    const fn default_max_retries() -> usize {
        3
    }

    const fn default_retry_delay() -> Duration {
        Duration::from_secs(5)
    }

    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *PublicKey::from(&PrivateKey::from(self.private_key)).as_bytes()
    }
}
