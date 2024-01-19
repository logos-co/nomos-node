use full_replication::{AbsoluteNumber, Attestation, Blob, Certificate, FullReplication};
use nomos_cli::{
    api::da::get_blobs,
    cmds::disseminate::{self, Disseminate},
    da::disseminate::{DaProtocolChoice, FullReplicationSettings, Protocol, ProtocolSettings},
};
use nomos_core::da::{blob::Blob as _, DaProtocol};
use std::{
    path::{self, PathBuf},
    time::Duration,
};
use tempfile::NamedTempFile;
use tests::{
    adjust_timeout, get_available_port, nodes::nomos::Pool, MixNode, Node, NomosNode, SpawnConfig,
};

const CLI_BIN: &str = "../target/debug/nomos-cli";

use std::process::Command;

const TIMEOUT_SECS: u64 = 20;

fn run_disseminate(disseminate: &Disseminate) {
    Command::new(CLI_BIN)
        .args(["disseminate", "--network-config"])
        .arg(disseminate.network_config.as_os_str())
        .args(["--data", &disseminate.data])
        .arg("--node-addr")
        .arg(disseminate.node_addr.as_ref().unwrap().as_str())
        .status()
        .expect("failed to execute nomos cli");
}

async fn disseminate(data: String) {
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
    let da_protocol = DaProtocolChoice {
        da_protocol: Protocol::FullReplication,
        settings: ProtocolSettings {
            full_replication: FullReplicationSettings {
                voter: [0; 32],
                num_attestations: 1,
            },
        },
    };

    let da =
        <FullReplication<AbsoluteNumber<Attestation, Certificate>>>::try_from(da_protocol.clone())
            .unwrap();
    let config = Disseminate {
        data: data.clone(),
        timeout: 20,
        network_config: config_path,
        da_protocol,
        node_addr: Some(
            format!(
                "http://{}",
                nodes[0].config().http.backend_settings.address.clone()
            )
            .parse()
            .unwrap(),
        ),
        output: None,
    };

    run_disseminate(&config);
    // let thread = std::thread::spawn(move || cmd.run().unwrap());

    tokio::time::timeout(
        adjust_timeout(Duration::from_secs(TIMEOUT_SECS)),
        wait_for_cert_in_mempool(&nodes[0]),
    )
    .await
    .unwrap();

    let blob = da.encode(data.as_bytes().to_vec())[0].hash();

    assert_eq!(
        get_blobs(&nodes[0].url(), vec![blob]).await.unwrap()[0].as_bytes(),
        data.as_bytes()
    );
}

#[tokio::test]
async fn disseminate_blob() {
    disseminate("hello world".to_string()).await;
}

#[tokio::test]
async fn disseminate_big_blob() {
    const MSG_SIZE: usize = 1024;
    disseminate(
        std::iter::repeat(String::from("X"))
            .take(MSG_SIZE)
            .collect::<Vec<_>>()
            .join(""),
    )
    .await;
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
