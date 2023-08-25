mod nodes;
use mixnet_node::MixnetNodeConfig;
use mixnet_topology::MixnetTopology;
pub use nodes::MixNode;
pub use nodes::NomosNode;

// std
use std::fmt::Debug;
use std::time::Duration;

//crates
use fraction::Fraction;

#[async_trait::async_trait]
pub trait Node: Sized {
    type ConsensusInfo: Debug + Clone + PartialEq;
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone)]
pub enum SpawnConfig {
    Star {
        n_participants: usize,
        threshold: Fraction,
        timeout: Duration,
        mixnet_node_configs: Vec<MixnetNodeConfig>,
        mixnet_topology: MixnetTopology,
    },
}

pub async fn run_nodes_and_destination_client() -> (
    MixnetTopology,
    impl futures::Stream<Item = Result<Vec<u8>, mixnet_client::MixnetClientError>> + Send,
) {
    use mixnet_client::{MixnetClient, MixnetClientConfig, MixnetClientMode};
    use mixnet_node::MixnetNode;
    use mixnet_topology::{Layer, Node};
    use std::{
        collections::HashMap,
        net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    };

    let config1 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        ..Default::default()
    };
    let config2 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        ..Default::default()
    };
    let config3 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 0)),
        ..Default::default()
    };

    let mixnode1 = MixnetNode::new(config1.clone());
    let mixnode2 = MixnetNode::new(config2.clone());
    let mixnode3 = MixnetNode::new(config3.clone());

    let topology = MixnetTopology {
        layers: vec![
            Layer {
                nodes: HashMap::from([(
                    mixnode1.id(),
                    Node {
                        address: config1.listen_address,
                        public_key: mixnode1.public_key(),
                    },
                )]),
            },
            Layer {
                nodes: HashMap::from([(
                    mixnode2.id(),
                    Node {
                        address: config2.listen_address,
                        public_key: mixnode2.public_key(),
                    },
                )]),
            },
            Layer {
                nodes: HashMap::from([(
                    mixnode3.id(),
                    Node {
                        address: config3.listen_address,
                        public_key: mixnode3.public_key(),
                    },
                )]),
            },
        ],
    };

    // Run all MixnetNodes
    tokio::spawn(async move {
        let res = mixnode1.run().await;
        assert!(res.is_ok());
    });
    tokio::spawn(async move {
        let res = mixnode2.run().await;
        assert!(res.is_ok());
    });
    tokio::spawn(async move {
        let res = mixnode3.run().await;
        assert!(res.is_ok());
    });

    // Wait until mixnodes are ready
    // TODO: use a more sophisticated way
    tokio::time::sleep(Duration::from_secs(1)).await;

    // Run a MixnetClient only for the MixnetNode in the exit layer.
    // According to the current implementation,
    // one of mixnodes the exit layer always will be selected as a destination.
    let client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::SenderReceiver(config3.client_listen_address),
            topology: topology.clone(),
        },
        rand::rngs::OsRng,
    );
    let client_stream = client.run().await.unwrap();

    (topology, client_stream)
}
