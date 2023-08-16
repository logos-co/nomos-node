use std::{
    collections::HashMap,
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
};

use mixnet_client::MixnetClient;
use mixnet_topology::{Layer, Mixnode, Topology};
use rand::{rngs::OsRng, RngCore};

#[tokio::test]
async fn mixnet() {
    let topology = run_mixnodes();
    let (client1, client2, destination) = run_clients(topology.clone()).await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let res = client1.send(msg.to_vec(), destination, &mut OsRng, topology.layers.len());
    assert!(res.is_ok());

    let received = client2.subscribe().recv().await.unwrap();
    assert_eq!(msg, received.as_slice());
}

fn run_mixnodes() -> Topology {
    let config1 = mixnode::config::Config {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7777)),
        ..Default::default()
    };
    let config2 = mixnode::config::Config {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7778)),
        ..Default::default()
    };
    let config3 = mixnode::config::Config {
        listen_address: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 7779)),
        ..Default::default()
    };

    let mixnode1 = mixnode::Mixnode::new(config1.clone());
    let mixnode2 = mixnode::Mixnode::new(config2.clone());
    let mixnode3 = mixnode::Mixnode::new(config3.clone());

    let topology = Topology {
        layers: vec![
            Layer {
                id: 0,
                nodes: HashMap::from([(
                    mixnode1.id(),
                    Mixnode {
                        address: config1.listen_address,
                        public_key: mixnode1.public_key(),
                    },
                )]),
            },
            Layer {
                id: 1,
                nodes: HashMap::from([(
                    mixnode2.id(),
                    Mixnode {
                        address: config2.listen_address,
                        public_key: mixnode2.public_key(),
                    },
                )]),
            },
            Layer {
                id: 2,
                nodes: HashMap::from([(
                    mixnode3.id(),
                    Mixnode {
                        address: config3.listen_address,
                        public_key: mixnode3.public_key(),
                    },
                )]),
            },
        ],
    };

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

    topology
}

async fn run_clients(topology: Topology) -> (MixnetClient, MixnetClient, SocketAddr) {
    let config1 = mixnet_client::config::Config {
        listen_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8777)),
        topology: topology.clone(),
    };
    let config2 = mixnet_client::config::Config {
        listen_addr: SocketAddr::V4(SocketAddrV4::new(Ipv4Addr::new(127, 0, 0, 1), 8778)),
        topology: topology.clone(),
    };

    let client1 = mixnet_client::MixnetClient::run(config1.clone())
        .await
        .unwrap();
    let client2 = mixnet_client::MixnetClient::run(config2.clone())
        .await
        .unwrap();

    (client1, client2, config2.listen_addr)
}
