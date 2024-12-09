use std::str::FromStr;

use nomos_libp2p::{ed25519, Multiaddr};
use nomos_mix::{conn_maintenance::ConnectionMaintenanceSettings, membership::Node};
use nomos_mix_message::{sphinx::SphinxMessage, MixMessage};
use nomos_mix_service::backends::libp2p::Libp2pMixBackendSettings;

use crate::get_available_port;

#[derive(Clone)]
pub struct GeneralMixConfig {
    pub backend: Libp2pMixBackendSettings,
    pub private_key: x25519_dalek::StaticSecret,
    pub membership: Vec<Node<Multiaddr, <SphinxMessage as MixMessage>::PublicKey>>,
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

    let nodes = mix_nodes(&configs);
    configs.iter_mut().for_each(|config| {
        config.membership = nodes.clone();
    });

    configs
}

fn mix_nodes(
    configs: &[GeneralMixConfig],
) -> Vec<Node<Multiaddr, <SphinxMessage as MixMessage>::PublicKey>> {
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
