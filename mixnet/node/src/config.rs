use std::net::{Ipv4Addr, SocketAddr, SocketAddrV4};

use nym_sphinx::PrivateKey;
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::PRIVATE_KEY_SIZE;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixnetNodeConfig {
    // A listen address for receiving Sphinx packets
    pub listen_address: SocketAddr,
    // A key for decrypting Sphinx packets
    pub private_key: [u8; PRIVATE_KEY_SIZE],
    // An address of the client with receiver-mode enabled, for sending final payloads
    pub client_address: SocketAddr,
}

impl Default for MixnetNodeConfig {
    fn default() -> Self {
        Self {
            listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(0, 0, 0, 0), 7777)),
            private_key: PrivateKey::new().to_bytes(),
            client_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7778)),
        }
    }
}
