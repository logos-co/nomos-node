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
use tokio::time::Instant;

#[tokio::test]
async fn mixnet_one_small_message() {
    // Similar size as Nomos messages.
    // But, this msg won't be splitted into multiple packets,
    // since min packet size is 2KB (hardcoded in nymsphinx).
    // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/params/src/packet_sizes.rs#L28C10-L28C10
    test_one_message(500).await
}

#[tokio::test]
async fn mixnet_one_message() {
    test_one_message(100 * 1024).await
}

async fn test_one_message(msg_size: usize) {
    let (topology, mut destination_stream) = run_nodes_and_destination_client().await;

    let mut msg = vec![0u8; msg_size];
    rand::thread_rng().fill_bytes(&mut msg);

    let mut sender_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::Sender,
            topology: topology.clone(),
        },
        OsRng,
    );

    let start_time = Instant::now();

    let res = sender_client.send(msg.to_vec());
    assert!(res.is_ok());

    let received = destination_stream.next().await.unwrap().unwrap();
    assert_eq!(msg, received.as_slice());

    let elapsed = Instant::now().checked_duration_since(start_time).unwrap();
    println!("ELAPSED: {elapsed:?}");
}

// TODO: This test should be enabled after the connection reuse is implemented:
//       https://github.com/logos-co/nomos-node/issues/313
//       Currently, this test fails with `Too many open files (os error 24)`.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn mixnet_ten_messages() {
    let (topology, mut destination_stream) = run_nodes_and_destination_client().await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let start_time = Instant::now();

    const NUM_MESSAGES: usize = 10;

    for _ in 0..NUM_MESSAGES {
        let msg = msg.clone();
        let topology = topology.clone();

        tokio::spawn(async move {
            let mut sender_client = MixnetClient::new(
                MixnetClientConfig {
                    mode: MixnetClientMode::Sender,
                    topology,
                },
                OsRng,
            );
            let res = sender_client.send(msg.to_vec());
            assert!(res.is_ok());
        });
    }

    for _ in 0..NUM_MESSAGES {
        let received = destination_stream.next().await.unwrap().unwrap();
        assert_eq!(msg, received.as_slice());
    }

    let elapsed = Instant::now().checked_duration_since(start_time).unwrap();
    println!("ELAPSED: {elapsed:?}");
}

async fn run_nodes_and_destination_client() -> (
    MixnetTopology,
    impl Stream<Item = Result<Vec<u8>, MixnetClientError>> + Send,
) {
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
    let client_stream = client.run().await.unwrap();

    (topology, client_stream)
}
