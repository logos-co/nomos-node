use nomos_cli::cmds::disseminate::Disseminate;
use nomos_cli::da::network::backend::ExecutorBackend;
use nomos_cli::da::network::backend::ExecutorBackendSettings;
use nomos_da_network_service::NetworkConfig;
use nomos_libp2p::ed25519;
use nomos_libp2p::libp2p;
use nomos_libp2p::Multiaddr;
use nomos_libp2p::PeerId;
use std::collections::HashMap;
use std::io::Write;
use subnetworks_assignations::versions::v1::FillFromNodeList;
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
        .arg("--columns")
        .arg(disseminate.columns.to_string())
        .arg("--timeout")
        .arg(disseminate.timeout.to_string())
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
    let nodes = NomosNode::spawn_nodes(SpawnConfig::star_happy(
        2,
        tests::DaConfig {
            dispersal_factor: 2,
            ..Default::default()
        },
    ))
    .await;

    // Nomos Cli is acting as the first node when dispersing the data by using the key associated
    // with that Nomos Node.
    let first_config = nodes[0].config();
    let node_key = first_config.da_network.backend.node_key.clone();
    let node_addrs: HashMap<PeerId, Multiaddr> = nodes
        .iter()
        .map(|n| {
            let libp2p_config = &n.config().network.backend.inner;
            let keypair = libp2p::identity::Keypair::from(ed25519::Keypair::from(
                libp2p_config.node_key.clone(),
            ));
            let peer_id = PeerId::from(keypair.public());
            let address = n
                .config()
                .da_network
                .backend
                .listening_address
                .clone()
                .with_p2p(peer_id)
                .unwrap();
            (peer_id, address)
        })
        .collect();
    let membership = first_config.da_network.backend.membership.clone();
    let num_subnets = first_config.da_sampling.sampling_settings.num_subnets;

    let da_network_config: NetworkConfig<ExecutorBackend<FillFromNodeList>> = NetworkConfig {
        backend: ExecutorBackendSettings {
            node_key,
            membership,
            node_addrs,
            num_subnets,
        },
    };

    let mut file = NamedTempFile::new().unwrap();
    let config_path = file.path().to_owned();
    serde_yaml::to_writer(&mut file, &da_network_config).unwrap();

    config.timeout = 180;
    config.network_config = config_path;
    config.node_addr = Some(
        format!(
            "http://{}",
            nodes[0].config().http.backend_settings.address.clone()
        )
        .parse()
        .unwrap(),
    );
    config.app_id = "fd3384e132ad02a56c78f45547ee40038dc79002b90d29ed90e08eee762ae715".to_string();
    config.index = 0;
    config.columns = 2;

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
