use std::net::SocketAddr;

use crate::get_available_port;

#[derive(Clone)]
pub struct GeneralApiConfig {
    pub address: SocketAddr,
}

pub fn create_api_configs(ids: &[[u8; 32]]) -> Vec<GeneralApiConfig> {
    ids.iter()
        .map(|_| GeneralApiConfig {
            address: format!("127.0.0.1:{}", get_available_port())
                .parse()
                .unwrap(),
        })
        .collect()
}
