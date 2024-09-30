// std
use std::{collections::HashMap, net::Ipv4Addr, str::FromStr};
// crates
use nomos_libp2p::{Multiaddr, PeerId};
use nomos_node::Config as NodeConfig;
use tests::{ConsensusConfig, DaConfig, Node, NomosNode};
// internal

const DEFAULT_NETWORK_PORT: u16 = 3000;
const DEFAULT_DA_NETWORK_PORT: u16 = 3300;

#[derive(Eq, PartialEq, Hash, Clone)]
pub enum HostKind {
    Nomos,
}

#[derive(Eq, PartialEq, Hash, Clone)]
pub struct Host {
    pub kind: HostKind,
    pub ip: Ipv4Addr,
    pub network_port: u16,
    pub da_network_port: u16,
}

impl Host {
    pub fn default_node_from_ip(ip: Ipv4Addr) -> Self {
        Self {
            kind: HostKind::Nomos,
            ip,
            network_port: DEFAULT_NETWORK_PORT,
            da_network_port: DEFAULT_DA_NETWORK_PORT,
        }
    }
}

pub fn create_node_configs(
    consensus: ConsensusConfig,
    da: DaConfig,
    hosts: Vec<Host>,
) -> HashMap<Host, NodeConfig> {
    let mut configs = NomosNode::create_node_configs(consensus, da);
    let mut configured_hosts = HashMap::new();

    // Rebuild DA address lists.
    let peer_addresses = configs[0].da_network.backend.addresses.clone();
    let host_network_init_peers = update_network_init_peers(hosts.clone());
    let host_da_peer_addresses = update_da_peer_addresses(hosts.clone(), peer_addresses);

    let new_peer_addresses: HashMap<PeerId, Multiaddr> = host_da_peer_addresses
        .clone()
        .into_iter()
        .map(|(peer_id, (multiaddr, _))| (peer_id, multiaddr))
        .collect();

    for (config, host) in configs.iter_mut().zip(hosts.into_iter()) {
        config.da_network.backend.addresses = new_peer_addresses.clone();

        // Libp2p network config.
        config.network.backend.inner.host = Ipv4Addr::from_str("0.0.0.0").unwrap();
        config.network.backend.inner.port = host.network_port;
        config.network.backend.initial_peers = host_network_init_peers.clone();

        // DA Libp2p network config.
        config.da_network.backend.listening_address = Multiaddr::from_str(&format!(
            "/ip4/0.0.0.0/udp/{}/quic-v1",
            host.da_network_port,
        ))
        .unwrap();

        configured_hosts.insert(host.clone(), config.clone());
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

#[cfg(test)]
mod cfgsync_tests {
    use std::str::FromStr;
    use std::{net::Ipv4Addr, time::Duration};

    use nomos_libp2p::Protocol;
    use tests::{ConsensusConfig, DaConfig};

    use super::{create_node_configs, Host, HostKind};

    #[test]
    fn basic_ip_list() {
        let hosts = (0..10)
            .map(|i| Host {
                kind: HostKind::Nomos,
                ip: Ipv4Addr::from_str(&format!("10.1.1.{i}")).unwrap(),
                network_port: 3000,
                da_network_port: 4044,
            })
            .collect();

        let configs = create_node_configs(
            ConsensusConfig {
                n_participants: 10,
                security_param: 10,
                active_slot_coeff: 0.9,
            },
            DaConfig {
                subnetwork_size: 2,
                dispersal_factor: 1,
                num_samples: 1,
                num_subnets: 2,
                old_blobs_check_interval: Duration::from_secs(5),
                blobs_validity_duration: Duration::from_secs(u64::MAX),
                global_params_path: "".into(),
            },
            hosts,
        );

        for (host, config) in configs.iter() {
            let network_port = config.network.backend.inner.port;

            let da_network_addr = config.da_network.backend.listening_address.clone();
            let da_network_port = da_network_addr
                .iter()
                .find_map(|protocol| match protocol {
                    Protocol::Udp(port) => Some(port),
                    _ => None,
                })
                .unwrap();

            assert_eq!(network_port, host.network_port);
            assert_eq!(da_network_port, host.da_network_port);
        }
    }
}
