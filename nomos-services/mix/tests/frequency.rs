use std::{str::FromStr, thread, time::Duration};

use libp2p::Multiaddr;
use nomos_libp2p::{ed25519, Swarm, SwarmConfig};
use nomos_mix::{
    conn_maintenance::ConnectionMaintenanceSettings,
    membership::Node,
    message_blend::{
        CryptographicProcessorSettings, MessageBlendSettings, TemporalSchedulerSettings,
    },
    persistent_transmission::PersistentTransmissionSettings,
};
use nomos_mix_message::{mock::MockMixMessage, MixMessage};
use nomos_mix_service::{
    self,
    backends::libp2p::{Libp2pMixBackend, Libp2pMixBackendSettings},
    network::libp2p::Libp2pAdapter,
    MixConfig, MixService,
};
use nomos_network::{
    backends::libp2p::{Libp2p, Libp2pConfig},
    NetworkConfig, NetworkService,
};
use overwatch_rs::{
    overwatch::{Overwatch, OverwatchRunner},
    services::handle::ServiceHandle,
    Services,
};

const NETWORK_SIZE: usize = 3;
const PEERING_DEGREE: usize = 2;

#[test]
fn emission_frequency() {
    let mix_configs = new_mix_configs(
        (0..NETWORK_SIZE)
            .map(|i| {
                Multiaddr::from_str(&format!("/ip4/127.0.0.1/udp/{}/quic-v1", 9990 + i)).unwrap()
            })
            .collect::<Vec<_>>(),
    );
    let network_backend_configs = new_network_backend_configs(
        &(0..NETWORK_SIZE)
            .map(|i| 9980 + i as u16)
            .collect::<Vec<_>>(),
    );

    let _nodes = mix_configs
        .iter()
        .zip(&network_backend_configs)
        .map(|(mix_config, network_backend_config)| new_node(mix_config, network_backend_config))
        .collect::<Vec<_>>();

    thread::sleep(Duration::from_secs(12));
}

fn new_node(mix_config: &TestMixSettings, network_backend_config: &Libp2pConfig) -> Overwatch {
    OverwatchRunner::<TestNode>::run(
        TestNodeServiceSettings {
            mix: MixConfig {
                backend: mix_config.backend.clone(),
                persistent_transmission: PersistentTransmissionSettings {
                    max_emission_frequency: 1.0,
                    drop_message_probability: 0.0,
                },
                message_blend: MessageBlendSettings {
                    cryptographic_processor: CryptographicProcessorSettings {
                        private_key: mix_config.private_key.to_bytes(),
                        num_mix_layers: 1,
                    },
                    temporal_processor: TemporalSchedulerSettings {
                        max_delay_seconds: 5,
                    },
                },
                cover_traffic: nomos_mix_service::CoverTrafficExtSettings {
                    epoch_duration: Duration::from_secs(10),
                    slot_duration: Duration::from_secs(1),
                },
                membership: mix_config.membership.clone(),
            },
            network: NetworkConfig {
                backend: network_backend_config.clone(),
            },
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {:?}", e))
    .unwrap()
}

#[derive(Services)]
struct TestNode {
    mix: ServiceHandle<MixService<Libp2pMixBackend, Libp2pAdapter>>,
    network: ServiceHandle<NetworkService<Libp2p>>,
}

struct TestMixSettings {
    pub backend: Libp2pMixBackendSettings,
    pub private_key: x25519_dalek::StaticSecret,
    pub membership: Vec<Node<<MockMixMessage as MixMessage>::PublicKey>>,
}

fn new_mix_configs(listening_addresses: Vec<Multiaddr>) -> Vec<TestMixSettings> {
    let settings = listening_addresses
        .iter()
        .map(|listening_address| {
            (
                Libp2pMixBackendSettings {
                    listening_address: listening_address.clone(),
                    node_key: ed25519::SecretKey::generate(),
                    peering_degree: PEERING_DEGREE,
                    max_peering_degree: PEERING_DEGREE + 5,
                    conn_maintenance: ConnectionMaintenanceSettings {
                        time_window: Duration::from_secs(10),
                        expected_effective_messages: 5.0,
                        effective_message_tolerance: 0.1,
                        expected_drop_messages: 0.0,
                        drop_message_tolerance: 0.0,
                    },
                },
                x25519_dalek::StaticSecret::random(),
            )
        })
        .collect::<Vec<_>>();

    let membership = settings
        .iter()
        .map(|(backend, private_key)| Node {
            address: backend.listening_address.clone(),
            public_key: x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(
                private_key.to_bytes(),
            ))
            .to_bytes(),
        })
        .collect::<Vec<_>>();

    settings
        .into_iter()
        .map(|(backend, private_key)| TestMixSettings {
            backend,
            private_key,
            membership: membership.clone(),
        })
        .collect()
}

fn new_network_backend_configs(ports: &[u16]) -> Vec<Libp2pConfig> {
    let swarm_configs = ports
        .iter()
        .map(|port| SwarmConfig {
            port: *port,
            ..Default::default()
        })
        .collect::<Vec<_>>();
    swarm_configs
        .iter()
        .enumerate()
        .map(|(i, swarm_config)| Libp2pConfig {
            inner: swarm_config.clone(),
            initial_peers: vec![node_address(&swarm_configs[(i + 1) % swarm_configs.len()])],
        })
        .collect()
}

fn node_address(config: &SwarmConfig) -> Multiaddr {
    Swarm::multiaddr(std::net::Ipv4Addr::new(127, 0, 0, 1), config.port)
}
