use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use futures::{Stream, StreamExt};
use mixnet_client::{MixnetClient, MixnetClientConfig, MixnetClientError, MixnetClientMode};
use mixnet_node::{MixnetNode, MixnetNodeConfig};
use mixnet_topology::{Layer, MixnetTopology, Node};
use rand::{rngs::OsRng, RngCore};
use tests::get_available_port;

#[tokio::test]
// Set timeout since the test won't stop even if mixnodes (spawned asynchronously) panic.
#[ntest::timeout(5000)]
async fn mixnet() {
    let (topology, mut destination_stream) = run_nodes_and_destination_client().await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let mut sender_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::Sender,
            topology: topology.clone(),
            connection_pool_size: 255,
        },
        OsRng,
    );

    let res = sender_client.send(msg.to_vec(), Duration::from_millis(500));
    assert!(res.is_ok());

    let received = destination_stream.next().await.unwrap().unwrap();
    assert_eq!(msg, received.as_slice());
}

async fn run_nodes_and_destination_client() -> (
    MixnetTopology,
    impl Stream<Item = Result<Vec<u8>, MixnetClientError>> + Send,
) {
    let config1 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
        ..Default::default()
    };
    let config2 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
        ..Default::default()
    };
    let config3 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(
            Ipv4Addr::new(127, 0, 0, 1),
            get_available_port(),
        )),
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
            connection_pool_size: 255,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
        },
        OsRng,
    );
    let client_stream = client.run().await.unwrap();

    (topology, client_stream)
}
