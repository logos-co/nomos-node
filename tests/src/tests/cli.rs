use nomos_cli::cmds::disseminate::Disseminate;
use std::io::Write;
use tempfile::NamedTempFile;
use tests::nodes::NomosNode;
use tests::Node;
use tests::SpawnConfig;

const CLI_BIN: &str = "../target/debug/nomos-cli";

use std::process::Command;

fn run_disseminate(disseminate: &Disseminate) {
    let mut binding = Command::new(CLI_BIN);
    let c = binding
        .args(["disseminate", "--network-config"])
        .arg(disseminate.network_config.as_os_str())
        .arg("--app-id")
        .arg(&disseminate.app_id)
        .arg("--index")
        .arg(disseminate.index.to_string())
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

    let mut file = NamedTempFile::new().unwrap();
    let config_path = file.path().to_owned();
    serde_yaml::to_writer(&mut file, &node_configs[1].network).unwrap();

    config.timeout = 20;
    config.network_config = config_path;
    config.node_addr = Some(
        format!(
            "http://{}",
            first_node.config().http.backend_settings.address.clone()
        )
        .parse()
        .unwrap(),
    );
    config.app_id = "fd3384e132ad02a56c78f45547ee40038dc79002b90d29ed90e08eee762ae715".to_string();
    config.index = 0;

    run_disseminate(&config);
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
