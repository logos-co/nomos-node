use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    time::Duration,
};

use mixnet_client::{MixnetClient, MixnetClientConfig, MixnetClientMode};
use mixnet_node::{MixnetNode, MixnetNodeConfig};
use mixnet_topology::{Layer, MixnetTopology, Node};
use rand::{rngs::OsRng, RngCore};
use tokio::sync::mpsc;
use tokio_util::sync::PollSender;

#[tokio::test]
async fn mixnet() {
    let (topology, mut destination_rx) = run_nodes_and_destination_client().await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let mut sender_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::Sender,
            topology: topology.clone(),
        },
        OsRng,
    );

    let res = sender_client.send(msg.to_vec(), Duration::from_millis(500));
    assert!(res.is_ok());

    let received = destination_rx.recv().await.unwrap();
    assert_eq!(msg, received.as_slice());
}

async fn run_nodes_and_destination_client() -> (MixnetTopology, mpsc::Receiver<Vec<u8>>) {
    let config1 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7777)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7778)),
        ..Default::default()
    };
    let config2 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8777)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8778)),
        ..Default::default()
    };
    let config3 = MixnetNodeConfig {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 9777)),
        client_listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 9778)),
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
        OsRng,
    );
    let (client_tx, client_rx) = mpsc::channel::<Vec<u8>>(1);
    tokio::spawn(async move {
        client.run(PollSender::new(client_tx)).await;
    });

    (topology, client_rx)
}
