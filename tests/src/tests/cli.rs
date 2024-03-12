use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication};
use nomos_cli::{
    api::da::get_blobs,
    cmds::disseminate::Disseminate,
    da::disseminate::{DaProtocolChoice, FullReplicationSettings, Protocol, ProtocolSettings},
};
use nomos_core::da::{blob::Blob as _, DaProtocol};
use std::{io::Write, time::Duration};
use tempfile::NamedTempFile;
use tests::{adjust_timeout, get_available_port, nodes::nomos::Pool, Node, NomosNode, SpawnConfig};

const CLI_BIN: &str = "../target/debug/nomos-cli";

use std::process::Command;

const TIMEOUT_SECS: u64 = 20;

fn run_disseminate(disseminate: &Disseminate) {
    let mut binding = Command::new(CLI_BIN);
    let c = binding
        .args(["disseminate", "--network-config"])
        .arg(disseminate.network_config.as_os_str())
        .arg("--node-addr")
        .arg(disseminate.node_addr.as_ref().unwrap().as_str());

    match (&disseminate.data, &disseminate.file) {
        (Some(data), None) => c.args(["--data", &data]),
        (None, Some(file)) => c.args(["--file", file.as_os_str().to_str().unwrap()]),
        (_, _) => panic!("Either data or file needs to be provided, but not both"),
    };

    c.status().expect("failed to execute nomos cli");
}

async fn disseminate(config: &mut Disseminate) {
    let node_configs = NomosNode::node_configs(SpawnConfig::chain_happy(2));
    let first_node = NomosNode::spawn(node_configs[0].clone()).await;

    let mut network_config = node_configs[1].network.clone();
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

    config.timeout = 20;
    config.network_config = config_path;
    config.da_protocol = da_protocol;
    config.node_addr = Some(
        format!(
            "http://{}",
            first_node.config().http.backend_settings.address.clone()
        )
        .parse()
        .unwrap(),
    );

    run_disseminate(&config);
    // let thread = std::thread::spawn(move || cmd.run().unwrap());

    tokio::time::timeout(
        adjust_timeout(Duration::from_secs(TIMEOUT_SECS)),
        wait_for_cert_in_mempool(&first_node),
    )
    .await
    .unwrap();

    let (blob, bytes) = if let Some(data) = &config.data {
        let bytes = data.as_bytes().to_vec();
        (da.encode(bytes.clone())[0].hash(), bytes)
    } else {
        let bytes = std::fs::read(&config.file.as_ref().unwrap()).unwrap();
        (da.encode(bytes.clone())[0].hash(), bytes)
    };

    assert_eq!(
        get_blobs(&first_node.url(), vec![blob]).await.unwrap()[0].as_bytes(),
        bytes.clone()
    );
}

#[tokio::test]
async fn disseminate_blob() {
    let mut config = Disseminate {
        data: Some("hello world".to_string()),
        ..Default::default()
    };
    disseminate(&mut config).await;
}

#[tokio::test]
async fn disseminate_big_blob() {
    const MSG_SIZE: usize = 1024;
    let mut config = Disseminate {
        data: std::iter::repeat(String::from("X"))
            .take(MSG_SIZE)
            .collect::<Vec<_>>()
            .join("")
            .into(),
        ..Default::default()
    };
    disseminate(&mut config).await;
}

#[tokio::test]
async fn disseminate_blob_from_file() {
    let mut file = NamedTempFile::new().unwrap();
    file.write_all("hello world".as_bytes()).unwrap();

    let mut config = Disseminate {
        file: Some(file.path().to_path_buf()),
        ..Default::default()
    };
    disseminate(&mut config).await;
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
