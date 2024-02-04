use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication};
use nomos_cli::{
    api::da::get_blobs,
    cmds::disseminate::Disseminate,
    da::disseminate::{DaProtocolChoice, FullReplicationSettings, Protocol, ProtocolSettings},
};
use nomos_core::da::{blob::Blob as _, DaProtocol};
use std::{io::Write, net::SocketAddr, path::PathBuf, time::Duration};
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
    file: &mut NamedTempFile,
    config: &mut Disseminate,
    storage_dir: Option<PathBuf>,
) -> (
    Vec<NomosNode>,
    FullReplication<AbsoluteNumber<Attestation, Certificate>>,
) {
    let (_mixnodes, mixnet_config) = MixNode::spawn_nodes(2).await;
    let mut nodes =
        NomosNode::spawn_nodes(SpawnConfig::chain_happy(2, mixnet_config), storage_dir).await;

    // kill the node so that we can reuse its network config
    nodes[1].stop();

    let mut network_config = nodes[1].config().network.clone();
    // use a new port because the old port is sometimes not closed immediately
    network_config.backend.inner.port = get_available_port();
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

    (nodes, da)
}

async fn disseminate(config: &mut Disseminate) {
    let mut file = NamedTempFile::new().unwrap();
    let (nodes, da) = spawn_and_setup_config(&mut file, config, None).await;
    run_disseminate(config);
    // let thread = std::thread::spawn(move || cmd.run().unwrap());

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

fn run_explorer(explorer_api_addr: SocketAddr, db_path: PathBuf) {
    let cfg = nomos_explorer::Config {
        log: Default::default(),
        api: nomos_api::ApiServiceSettings {
            backend_settings: nomos_explorer::AxumBackendSettings {
                address: explorer_api_addr,
                cors_origins: Vec::new(),
            },
        },
        storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
            db_path,
            read_only: true,
            column_family: Some("blocks".into()),
        },
    };
    std::thread::spawn(move || nomos_explorer::Explorer::run(cfg).unwrap());
}

#[test]
fn explorer() {
    let (tx, rx) = std::sync::mpsc::channel();
    let tx1 = tx.clone();
    let temp = tempfile::tempdir().unwrap();
    let db_path = temp.path().to_path_buf();

    std::thread::spawn(move || {
        tokio::runtime::Runtime::new()
            .unwrap()
            .block_on(async move {
                let mut file = NamedTempFile::new().unwrap();
                let mut config = Disseminate::default();
                let (nodes, _) =
                    spawn_and_setup_config(&mut file, &mut config, Some(db_path)).await;
                let explorer_db = nodes[0].config().storage.db_path.clone();

                let explorer_api_addr = format!("127.0.0.1:{}", get_available_port())
                    .parse()
                    .unwrap();
                run_explorer(explorer_api_addr, explorer_db);

                let c = nomos_cli::cmds::chat::NomosChat {
                    network_config: config.network_config.clone(),
                    da_protocol: config.da_protocol.clone(),
                    node: config.node_addr.clone().unwrap(),
                    explorer: format!("http://{}", explorer_api_addr).parse().unwrap(),
                    message: None,
                    author: None,
                };
                let c1 = c.clone();

                std::thread::Builder::new()
                    .name("user1".into())
                    .spawn(move || {
                        let (rx1, app1) = c.run_app_without_terminal("user1".into()).unwrap();
                        app1.send_message("Hello from user1".into());
                        let msgs1 = rx1.recv().unwrap();
                        assert!(!msgs1.is_empty());
                        tx1.send(()).unwrap();
                    })
                    .unwrap();

                std::thread::Builder::new()
                    .name("user2".into())
                    .spawn(move || {
                        let (rx2, app2) = c1.run_app_without_terminal("user2".into()).unwrap();
                        app2.send_message("Hello from user2".into());
                        let msgs2 = rx2.recv().unwrap();
                        assert!(!msgs2.is_empty());
                        tx.send(()).unwrap();
                    })
                    .unwrap();
            });
    });

    assert!(rx.recv().is_ok());
    assert!(rx.recv().is_ok());
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
