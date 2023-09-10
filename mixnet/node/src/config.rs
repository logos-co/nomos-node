use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use nym_sphinx::{PrivateKey, PublicKey};
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::{PRIVATE_KEY_SIZE, PUBLIC_KEY_SIZE};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetNodeConfig {
    /// A listen address for receiving Sphinx packets
    pub listen_address: SocketAddr,
    /// An listen address fro communicating with mixnet clients
    pub client_listen_address: SocketAddr,
    /// A key for decrypting Sphinx packets
    pub private_key: [u8; PRIVATE_KEY_SIZE],
    /// The size of the connection pool.
    pub connection_pool_size: usize,
    /// The maximum number of retries.
    pub max_retries: usize,
    /// The retry delay between retries.
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
    pub fn public_key(&self) -> [u8; PUBLIC_KEY_SIZE] {
        *PublicKey::from(&PrivateKey::from(self.private_key)).as_bytes()
    }
}
