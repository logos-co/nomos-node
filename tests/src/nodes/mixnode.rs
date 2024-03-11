use std::{
    net::{Ipv4Addr, SocketAddr, SocketAddrV4},
    process::{Child, Command, Stdio},
    time::Duration,
};

use super::{create_tempdir, persist_tempdir, LOGS_PREFIX};
use mixnet_node::{MixnetNodeConfig, PRIVATE_KEY_SIZE};
use mixnet_topology::{Layer, MixnetTopology, Node};
use nomos_log::{LoggerBackend, LoggerFormat};
use rand::{thread_rng, RngCore};
use tempfile::NamedTempFile;

use crate::{get_available_port, MixnetConfig};

const MIXNODE_BIN: &str = "../target/debug/mixnode";

pub struct MixNode {
    _tempdir: tempfile::TempDir,
    child: Child,
}

impl Drop for MixNode {
    fn drop(&mut self) {
        if std::thread::panicking() {
            if let Err(e) = persist_tempdir(&mut self._tempdir, "mixnode") {
                println!("failed to persist tempdir: {e}");
            }
        }

        if let Err(e) = self.child.kill() {
            println!("failed to kill the child process: {e}");
        }
    }
}

impl MixNode {
    pub async fn spawn(config: MixnetNodeConfig) -> Self {
        let dir = create_tempdir().unwrap();

        let mut config = mixnode::Config {
            mixnode: config,
            log: Default::default(),
        };
        config.log.backend = LoggerBackend::File {
            directory: dir.path().to_owned(),
            prefix: Some(LOGS_PREFIX.into()),
        };
        config.log.format = LoggerFormat::Json;

        let mut file = NamedTempFile::new().unwrap();
        let config_path = file.path().to_owned();
        serde_yaml::to_writer(&mut file, &config).unwrap();

        let child = Command::new(std::env::current_dir().unwrap().join(MIXNODE_BIN))
            .arg(&config_path)
            .stdout(Stdio::null())
            .spawn()
            .unwrap();

        //TODO: use a sophisticated way to wait until the node is ready
        tokio::time::sleep(Duration::from_secs(1)).await;

        Self {
            _tempdir: dir,
            child,
        }
    }

    pub async fn spawn_nodes(num_nodes: usize) -> (Vec<Self>, MixnetConfig) {
        let mut configs = Vec::<MixnetNodeConfig>::new();
        for _ in 0..num_nodes {
            let mut private_key = [0u8; PRIVATE_KEY_SIZE];
            thread_rng().fill_bytes(&mut private_key);

            let config = MixnetNodeConfig {
                listen_address: SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(127, 0, 0, 1),
                    get_available_port(),
                )),
                client_listen_address: SocketAddr::V4(SocketAddrV4::new(
                    Ipv4Addr::new(127, 0, 0, 1),
                    get_available_port(),
                )),
                private_key,
                connection_pool_size: 255,
                ..Default::default()
            };
            configs.push(config);
        }

        let mut nodes = Vec::<MixNode>::new();
        for config in &configs {
            println!(
                "mixnode client {} mixnode listener {}",
                config.client_listen_address, config.listen_address
            );
            nodes.push(Self::spawn(*config).await);
        }

        // We need to return configs as well, to configure mixclients accordingly
        (
            nodes,
            MixnetConfig {
                node_configs: configs.clone(),
                topology: Self::build_topology(configs),
            },
        )
    }

    fn build_topology(configs: Vec<MixnetNodeConfig>) -> MixnetTopology {
        // Build three empty layers first
        let mut layers = vec![Layer { nodes: Vec::new() }; 3];
        let mut layer_id = 0;

        // Assign nodes to each layer in round-robin
        for config in &configs {
            let public_key = config.public_key();
            layers.get_mut(layer_id).unwrap().nodes.push(Node {
                address: config.listen_address,
                public_key,
            });
            layer_id = (layer_id + 1) % layers.len();
        }

        // Exclude empty layers
        MixnetTopology {
            layers: layers
                .iter()
                .filter(|layer| !layer.nodes.is_empty())
                .cloned()
                .collect(),
        }
    }
}
