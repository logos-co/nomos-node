// std
use std::{collections::HashMap, net::Ipv4Addr, str::FromStr};
// crates
use nomos_libp2p::{Multiaddr, PeerId};
use nomos_mix::membership::Node;
use nomos_mix_message::{mock::MockMixMessage, MixMessage};
use nomos_tracing::{
    logging::loki::LokiConfig, metrics::otlp::OtlpMetricsConfig, tracing::otlp::OtlpTracingConfig,
};
use nomos_tracing_service::{FilterLayer, LoggerLayer, MetricsLayer, TracingSettings};
use rand::{thread_rng, Rng};
use tests::topology::configs::{
    api::GeneralApiConfig,
    consensus::{create_consensus_configs, ConsensusParams},
    da::{create_da_configs, DaParams},
    mix::create_mix_configs,
    network::create_network_configs,
    tracing::GeneralTracingConfig,
    GeneralConfig,
};
use tracing::Level;
// internal
use crate::TracingParams;

const DEFAULT_LIBP2P_NETWORK_PORT: u16 = 3000;
const DEFAULT_DA_NETWORK_PORT: u16 = 3300;
const DEFAULT_MIX_PORT: u16 = 3400;
const DEFAULT_API_PORT: u16 = 18080;

#[derive(Eq, PartialEq, Hash, Clone)]
pub enum HostKind {
    Validator,
    Executor,
}

#[derive(Eq, PartialEq, Hash, Clone)]
pub struct Host {
    pub kind: HostKind,
    pub ip: Ipv4Addr,
    pub identifier: String,
    pub network_port: u16,
    pub da_network_port: u16,
    pub mix_port: u16,
}

impl Host {
    pub fn default_validator_from_ip(ip: Ipv4Addr, identifier: String) -> Self {
        Self {
            kind: HostKind::Validator,
            ip,
            identifier,
            network_port: DEFAULT_LIBP2P_NETWORK_PORT,
            da_network_port: DEFAULT_DA_NETWORK_PORT,
            mix_port: DEFAULT_MIX_PORT,
        }
    }

    pub fn default_executor_from_ip(ip: Ipv4Addr, identifier: String) -> Self {
        Self {
            kind: HostKind::Executor,
            ip,
            identifier,
            network_port: DEFAULT_LIBP2P_NETWORK_PORT,
            da_network_port: DEFAULT_DA_NETWORK_PORT,
            mix_port: DEFAULT_MIX_PORT,
        }
    }
}

pub fn create_node_configs(
    consensus_params: ConsensusParams,
    da_params: DaParams,
    tracing_params: TracingParams,
    hosts: Vec<Host>,
) -> HashMap<Host, GeneralConfig> {
    let mut ids = vec![[0; 32]; consensus_params.n_participants];
    for id in &mut ids {
        thread_rng().fill(id);
    }

    let consensus_configs = create_consensus_configs(&ids, consensus_params);
    let da_configs = create_da_configs(&ids, da_params);
    let network_configs = create_network_configs(&ids, Default::default());
    let mix_configs = create_mix_configs(&ids);
    let api_configs = ids
        .iter()
        .map(|_| GeneralApiConfig {
            address: format!("0.0.0.0:{DEFAULT_API_PORT}").parse().unwrap(),
        })
        .collect::<Vec<_>>();
    let mut configured_hosts = HashMap::new();

    // Rebuild DA address lists.
    let peer_addresses = da_configs[0].addresses.clone();
    let host_network_init_peers = update_network_init_peers(hosts.clone());
    let host_da_peer_addresses = update_da_peer_addresses(hosts.clone(), peer_addresses);
    let host_mix_membership =
        update_mix_membership(hosts.clone(), mix_configs[0].membership.clone());

    let new_peer_addresses: HashMap<PeerId, Multiaddr> = host_da_peer_addresses
        .clone()
        .into_iter()
        .map(|(peer_id, (multiaddr, _))| (peer_id, multiaddr))
        .collect();

    for (i, host) in hosts.into_iter().enumerate() {
        let consensus_config = consensus_configs[i].to_owned();
        let api_config = api_configs[i].to_owned();

        // DA Libp2p network config.
        let mut da_config = da_configs[i].to_owned();
        da_config.addresses = new_peer_addresses.clone();
        da_config.listening_address = Multiaddr::from_str(&format!(
            "/ip4/0.0.0.0/udp/{}/quic-v1",
            host.da_network_port,
        ))
        .unwrap();

        // Libp2p network config.
        let mut network_config = network_configs[i].to_owned();
        network_config.swarm_config.host = Ipv4Addr::from_str("0.0.0.0").unwrap();
        network_config.swarm_config.port = host.network_port;
        network_config.initial_peers = host_network_init_peers.clone();

        // Mix config.
        let mut mix_config = mix_configs[i].to_owned();
        mix_config.backend.listening_address =
            Multiaddr::from_str(&format!("/ip4/0.0.0.0/udp/{}/quic-v1", host.mix_port)).unwrap();
        mix_config.membership = host_mix_membership.clone();

        // Tracing config.
        let tracing_config =
            tracing_config_for_grafana(tracing_params.clone(), host.identifier.clone());

        configured_hosts.insert(
            host.clone(),
            GeneralConfig {
                consensus_config,
                da_config,
                network_config,
                mix_config,
                api_config,
                tracing_config,
            },
        );
    }

    configured_hosts
}

fn update_network_init_peers(hosts: Vec<Host>) -> Vec<Multiaddr> {
    hosts
        .iter()
        .map(|h| nomos_libp2p::Swarm::multiaddr(h.ip, h.network_port))
        .collect()
}

fn update_da_peer_addresses(
    hosts: Vec<Host>,
    peer_addresses: HashMap<PeerId, Multiaddr>,
) -> HashMap<PeerId, (Multiaddr, Ipv4Addr)> {
    peer_addresses
        .into_iter()
        .zip(hosts)
        .map(|((peer_id, _), host)| {
            let new_multiaddr = Multiaddr::from_str(&format!(
                "/ip4/{}/udp/{}/quic-v1",
                host.ip, host.da_network_port,
            ))
            .unwrap();

            (peer_id, (new_multiaddr, host.ip))
        })
        .collect()
}

fn update_mix_membership(
    hosts: Vec<Host>,
    membership: Vec<Node<<MockMixMessage as MixMessage>::PublicKey>>,
) -> Vec<Node<<MockMixMessage as MixMessage>::PublicKey>> {
    membership
        .into_iter()
        .zip(hosts)
        .map(|(mut node, host)| {
            node.address =
                Multiaddr::from_str(&format!("/ip4/{}/udp/{}/quic-v1", host.ip, host.mix_port))
                    .unwrap();
            node
        })
        .collect()
}

fn tracing_config_for_grafana(params: TracingParams, identifier: String) -> GeneralTracingConfig {
    GeneralTracingConfig {
        tracing_settings: TracingSettings {
            logger: LoggerLayer::Loki(LokiConfig {
                endpoint: params.loki_endpoint,
                host_identifier: identifier.clone(),
            }),
            tracing: nomos_tracing_service::TracingLayer::Otlp(OtlpTracingConfig {
                endpoint: params.tempo_endpoint,
                sample_ratio: 1.0,
            }),
            filter: FilterLayer::None,
            metrics: MetricsLayer::Otlp(OtlpMetricsConfig {
                endpoint: "http://127.0.0.1:9090/api/v1/otlp/v1/metrics"
                    .try_into()
                    .unwrap(),
                host_identifier: identifier,
            }),
            level: Level::INFO,
        },
    }
}

#[cfg(test)]
mod cfgsync_tests {
    use std::str::FromStr;
    use std::{net::Ipv4Addr, time::Duration};

    use nomos_libp2p::{Multiaddr, Protocol};
    use tests::topology::configs::consensus::ConsensusParams;
    use tests::topology::configs::da::DaParams;

    use crate::TracingParams;

    use super::{create_node_configs, Host, HostKind};

    #[test]
    fn basic_ip_list() {
        let hosts = (0..10)
            .map(|i| Host {
                kind: HostKind::Validator,
                ip: Ipv4Addr::from_str(&format!("10.1.1.{i}")).unwrap(),
                identifier: "node".into(),
                network_port: 3000,
                da_network_port: 4044,
                mix_port: 5000,
            })
            .collect();

        let configs = create_node_configs(
            ConsensusParams {
                n_participants: 10,
                security_param: 10,
                active_slot_coeff: 0.9,
            },
            DaParams {
                subnetwork_size: 2,
                dispersal_factor: 1,
                num_samples: 1,
                num_subnets: 2,
                old_blobs_check_interval: Duration::from_secs(5),
                blobs_validity_duration: Duration::from_secs(u64::MAX),
                global_params_path: "".into(),
            },
            TracingParams {
                tempo_endpoint: "http://test.com".try_into().unwrap(),
                loki_endpoint: "http://test.com".try_into().unwrap(),
                metrics_endpoint: "http://test.com".try_into().unwrap(),
            },
            hosts,
        );

        for (host, config) in configs.iter() {
            let network_port = config.network_config.swarm_config.port;
            let da_network_port = extract_port(&config.da_config.listening_address);
            let mix_port = extract_port(&config.mix_config.backend.listening_address);

            assert_eq!(network_port, host.network_port);
            assert_eq!(da_network_port, host.da_network_port);
            assert_eq!(mix_port, host.mix_port);
        }
    }

    fn extract_port(multiaddr: &Multiaddr) -> u16 {
        multiaddr
            .iter()
            .find_map(|protocol| match protocol {
                Protocol::Udp(port) => Some(port),
                _ => None,
            })
            .unwrap()
    }
}
