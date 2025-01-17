use std::str::FromStr;

use nomos_blend::{conn_monitor::ConnectionMaintenanceSettings, membership::Node};
use nomos_blend_message::{sphinx::SphinxMessage, BlendMessage};
use nomos_blend_service::backends::libp2p::Libp2pBlendBackendSettings;
use nomos_libp2p::{ed25519, Multiaddr};

use crate::get_available_port;

#[derive(Clone)]
pub struct GeneralBlendConfig {
    pub backend: Libp2pBlendBackendSettings,
    pub private_key: x25519_dalek::StaticSecret,
    pub membership: Vec<Node<<SphinxMessage as BlendMessage>::PublicKey>>,
}

pub fn create_blend_configs(ids: &[[u8; 32]]) -> Vec<GeneralBlendConfig> {
    let mut configs: Vec<GeneralBlendConfig> = ids
        .iter()
        .map(|id| {
            let mut node_key_bytes = *id;
            let node_key = ed25519::SecretKey::try_from_bytes(&mut node_key_bytes)
                .expect("Failed to generate secret key from bytes");

            GeneralBlendConfig {
                backend: Libp2pBlendBackendSettings {
                    listening_address: Multiaddr::from_str(&format!(
                        "/ip4/127.0.0.1/udp/{}/quic-v1",
                        get_available_port(),
                    ))
                    .unwrap(),
                    node_key,
                    conn_maintenance: ConnectionMaintenanceSettings {
                        peering_degree: 1,
                        max_peering_degree: 3,
                        monitor: None,
                    },
                },
                private_key: x25519_dalek::StaticSecret::random(),
                membership: Vec::new(),
            }
        })
        .collect();

    let nodes = blend_nodes(&configs);
    configs.iter_mut().for_each(|config| {
        config.membership = nodes.clone();
    });

    configs
}

fn blend_nodes(
    configs: &[GeneralBlendConfig],
) -> Vec<Node<<SphinxMessage as BlendMessage>::PublicKey>> {
    configs
        .iter()
        .map(|config| Node {
            address: config.backend.listening_address.clone(),
            public_key: x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
                config.private_key.to_bytes(),
            ))
            .to_bytes(),
        })
        .collect()
}
