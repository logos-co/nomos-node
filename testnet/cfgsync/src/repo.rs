// std
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
// crates
use nomos_node::Config as NodeConfig;
use serde::{Deserialize, Serialize};
use tests::{ConsensusConfig, DaConfig};
use tokio::sync::oneshot::Sender;
use tokio::time::timeout;
// internal

use crate::config::{create_node_configs, Host};

#[derive(Serialize, Deserialize)]
pub enum RepoResponse {
    Config(NodeConfig),
    Timeout,
}

pub struct ConfigRepo {
    waiting_hosts: Mutex<HashMap<Host, Sender<RepoResponse>>>,
    n_hosts: usize,
    consensus_config: ConsensusConfig,
    da_config: DaConfig,
    timeout_duration: Duration,
}

impl ConfigRepo {
    pub fn new(
        n_hosts: usize,
        consensus_config: ConsensusConfig,
        da_config: DaConfig,
        timeout_duration: Duration,
    ) -> Arc<Self> {
        let repo = Arc::new(Self {
            waiting_hosts: Mutex::new(HashMap::new()),
            n_hosts,
            consensus_config,
            da_config,
            timeout_duration,
        });

        let repo_clone = repo.clone();
        tokio::spawn(async move {
            repo_clone.run().await;
        });

        repo
    }

    pub fn register(&self, host: Host, reply_tx: Sender<RepoResponse>) {
        let mut waiting_hosts = self.waiting_hosts.lock().unwrap();
        waiting_hosts.insert(host, reply_tx);
    }

    async fn run(&self) {
        let timeout_duration = self.timeout_duration;

        match timeout(timeout_duration, self.wait_for_hosts()).await {
            Ok(_) => {
                println!("All hosts have announced their IPs");

                let mut waiting_hosts = self.waiting_hosts.lock().unwrap();
                let hosts = waiting_hosts
                    .iter()
                    .map(|(host, _)| host)
                    .cloned()
                    .collect();

                let configs = create_node_configs(
                    self.consensus_config.clone(),
                    self.da_config.clone(),
                    hosts,
                );

                for (host, sender) in waiting_hosts.drain() {
                    let config = configs.get(&host).expect("host should have a config");
                    let _ = sender.send(RepoResponse::Config(config.to_owned()));
                }
            }
            Err(_) => {
                println!("Timeout: Not all hosts announced within the time limit");

                let mut waiting_hosts = self.waiting_hosts.lock().unwrap();
                for (_, sender) in waiting_hosts.drain() {
                    let _ = sender.send(RepoResponse::Timeout);
                }
            }
        }
    }

    async fn wait_for_hosts(&self) {
        loop {
            if self.waiting_hosts.lock().unwrap().len() >= self.n_hosts {
                break;
            }
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
        }
    }
}
