use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use mixnet_client::{MixnetClient, MixnetClientConfig};
use mixnet_node::{MixnetNode, MixnetNodeConfig};
use mixnet_topology::{Layer, MixnetTopology, Node};
use rand::{rngs::OsRng, RngCore};

#[tokio::test]
async fn mixnet() {
    let (topology, destination_client) = run_nodes_and_clients().await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let sender_client = MixnetClient::run(MixnetClientConfig {
        listen_address: None, // It's not mandatory for senders to listen conns from MixnetNode
        topology: topology.clone(),
    })
    .await
    .unwrap();

    let res = sender_client.send(msg.to_vec(), &mut OsRng);
    assert!(res.is_ok());

    let received = destination_client.subscribe().recv().await.unwrap();
    assert_eq!(msg, received.as_slice());
}

async fn run_nodes_and_clients() -> (MixnetTopology, MixnetClient) {
    let config1 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7777)),
        client_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7778)),
        ..Default::default()
    };
    let config2 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8777)),
        client_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8778)),
        ..Default::default()
    };
    let config3 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 9777)),
        client_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 9778)),
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

    // Run MixnetClient for each MixnetNode
    let _ = MixnetClient::run(MixnetClientConfig {
        topology: topology.clone(),
        listen_address: Some(config1.client_address),
    })
    .await
    .unwrap();
    let _ = MixnetClient::run(MixnetClientConfig {
        topology: topology.clone(),
        listen_address: Some(config2.client_address),
    })
    .await
    .unwrap();
    let client3 = MixnetClient::run(MixnetClientConfig {
        topology: topology.clone(),
        listen_address: Some(config3.client_address),
    })
    .await
    .unwrap();

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

    // According to the current implementation,
    // the client3 (connected with the mixnode3 in the exit layer) always will be selected
    // as a destination.
    (topology, client3)
}
