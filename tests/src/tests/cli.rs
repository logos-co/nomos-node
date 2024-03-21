use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication};
use futures::{stream, StreamExt};
use nomos_cli::{
    api::da::get_blobs,
    cmds::disseminate::Disseminate,
    da::disseminate::{DaProtocolChoice, FullReplicationSettings, Protocol, ProtocolSettings},
};
use nomos_core::block::Block;
use nomos_core::da::{blob::Blob as _, DaProtocol};
use nomos_network::{backends::libp2p::Libp2p, NetworkConfig};
use nomos_node::Tx;
use std::{collections::HashSet, io::Write, net::SocketAddr, path::PathBuf, time::Duration};
use tempfile::NamedTempFile;
use tests::{
    adjust_timeout, get_available_port, nodes::nomos::Pool, MixNode, Node, NomosNode, SpawnConfig,
};

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

async fn spawn_and_setup_config(
    num_mixnodes: usize,
    num_nodes: usize,
    file: &mut NamedTempFile,
    config: &mut Disseminate,
    network_config: Option<NetworkConfig<Libp2p>>,
    storage_dir: Option<PathBuf>,
) -> (
    Vec<MixNode>,
    Vec<NomosNode>,
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
) {
    let (mixnodes, mixnet_config) = MixNode::spawn_nodes(num_mixnodes).await;
    let mut nodes = NomosNode::spawn_nodes(
        SpawnConfig::chain_happy(num_nodes, mixnet_config),
        storage_dir,
    )
    .await;

    let network_config = if let Some(nc) = network_config {
        nc
    } else {
        // kill the node so that we can reuse its network config
        nodes[1].stop();

        let mut network_config = nodes[1].config().network.clone();
        // use a new port because the old port is sometimes not closed immediately
        network_config.backend.inner.port = get_available_port();
        network_config
    };

    let config_path = file.path().to_owned();
    serde_yaml::to_writer(file, &network_config).unwrap();
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
            nodes[0].config().http.backend_settings.address.clone()
        )
        .parse()
        .unwrap(),
    );

    (mixnodes, nodes, da)
}

async fn disseminate(config: &mut Disseminate) {
    let mut file = NamedTempFile::new().unwrap();
    let (_mixnodes, nodes, da) = spawn_and_setup_config(2, 2, &mut file, config, None, None).await;
    run_disseminate(config);

    tokio::time::timeout(
        adjust_timeout(Duration::from_secs(TIMEOUT_SECS)),
        wait_for_cert_in_mempool(&nodes[0]),
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
        get_blobs(&nodes[0].url(), vec![blob]).await.unwrap()[0].as_bytes(),
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
