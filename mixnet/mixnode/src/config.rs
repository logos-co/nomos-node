use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use nym_sphinx::PrivateKey;
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::PRIVATE_KEY_SIZE;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct Config {
    pub listen_address: SocketAddr,
    pub private_key: [u8; PRIVATE_KEY_SIZE],
}

impl Default for Config {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 7777)),
            private_key: PrivateKey::new().to_bytes(),
        }
    }
}
