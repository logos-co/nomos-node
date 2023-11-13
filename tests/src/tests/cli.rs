use nomos_cli::{
    cmds::{disseminate::Disseminate, Command},
    da::disseminate::{DaProtocolChoice, FullReplicationSettings, Protocol, ProtocolSettings},
};
use std::time::Duration;
use tempfile::NamedTempFile;
use tests::{get_available_port, nodes::nomos::Pool, MixNode, Node, NomosNode, SpawnConfig};

const TIMEOUT_SECS: u64 = 20;

#[tokio::test]
async fn disseminate_blob() {
    let (_mixnodes, mixnet_config) = MixNode::spawn_nodes(2).await;
    let mut nodes = NomosNode::spawn_nodes(SpawnConfig::chain_happy(2, mixnet_config)).await;

    // kill the node so that we can reuse its network config
    nodes[1].stop();

    let mut network_config = nodes[1].config().network.clone();
    // use a new port because the old port is sometimes not closed immediately
    network_config.backend.inner.port = get_available_port();

    let mut file = NamedTempFile::new().unwrap();
    let config_path = file.path().to_owned();
    serde_yaml::to_writer(&mut file, &network_config).unwrap();
    let cmd = Command::Disseminate(Disseminate {
        data: "Hello World".into(),
        timeout: 20,
        network_config: config_path,
        da_protocol: DaProtocolChoice {
            da_protocol: Protocol::FullReplication,
            settings: ProtocolSettings {
                full_replication: FullReplicationSettings {
                    voter: [0; 32],
                    num_attestations: 1,
                },
            },
        },
        node_addr: Some(
            format!(
                "http://{}",
                nodes[0].config().http.backend_settings.address.clone()
            )
            .parse()
            .unwrap(),
        ),
        output: None,
    });

    let thread = std::thread::spawn(move || cmd.run().unwrap());

    tokio::time::timeout(
        Duration::from_secs(TIMEOUT_SECS),
        wait_for_cert_in_mempool(&nodes[0]),
    )
    .await
    .unwrap();

    thread.join().unwrap();
}

async fn wait_for_cert_in_mempool(node: &NomosNode) {
    loop {
        if node
            .get_mempoool_metrics(Pool::Da)
            .await
            .last_item_timestamp
            != 0
        {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
