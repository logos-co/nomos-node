use std::str::FromStr;

use nomos_libp2p::{ed25519, Multiaddr};
use nomos_mix_service::backends::libp2p::Libp2pMixBackendSettings;

use crate::{get_available_port, secret_key_to_peer_id};

#[derive(Clone)]
pub struct GeneralMixConfig {
    pub backend: Libp2pMixBackendSettings,
}

pub fn create_mix_configs(ids: &[[u8; 32]]) -> Vec<GeneralMixConfig> {
    let mut configs: Vec<GeneralMixConfig> = ids
        .iter()
        .map(|id| {
            let mut node_key_bytes = *id;
            let node_key = ed25519::SecretKey::try_from_bytes(&mut node_key_bytes)
                .expect("Failed to generate secret key from bytes");

            GeneralMixConfig {
                backend: Libp2pMixBackendSettings {
                    listening_address: Multiaddr::from_str(&format!(
                        "/ip4/127.0.0.1/udp/{}/quic-v1",
                        get_available_port(),
                    ))
                    .unwrap(),
                    node_key,
                    membership: Vec::new(),
                    peering_degree: 1,
                    num_mix_layers: 1,
                },
            }
        })
        .collect();

    let membership = mix_membership(&configs);

    configs.iter_mut().for_each(|config| {
        config.backend.membership = membership.clone();
    });

    configs
}

fn mix_membership(configs: &[GeneralMixConfig]) -> Vec<Multiaddr> {
    configs
        .iter()
        .map(|config| {
            let peer_id = secret_key_to_peer_id(config.backend.node_key.clone());
            config
                .backend
                .listening_address
                .clone()
                .with_p2p(peer_id)
                .unwrap_or_else(|orig_addr| orig_addr)
        })
        .collect()
}
