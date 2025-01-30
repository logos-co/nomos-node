use cfgsync::CfgSyncConfig;
use std::process::{Child, Command, Stdio};
use std::time::Duration;
use std::{net::SocketAddr, path::PathBuf};
use tempfile::NamedTempFile;
use tokio::time::timeout;

const CFGSYNC_BIN_PATH: &str = "../target/debug/cfgsync-server";

pub struct CfgSync {
    addr: SocketAddr,
    tempdir: tempfile::TempDir,
    child: Child,
    config_path: PathBuf,
}

impl Drop for CfgSync {
    fn drop(&mut self) {
        if std::thread::panicking() {
            println!("CfgSync process crashed; preserving tempdir");
        }

        if let Err(e) = self.child.kill() {
            println!("Failed to kill CfgSync process: {e}");
        }
    }
}

impl CfgSync {
    pub async fn spawn(config: CfgSyncConfig) -> Self {
        let dir = tempfile::tempdir().expect("Failed to create tempdir");
        let mut file = NamedTempFile::new().expect("Failed to create temp file");
        let config_path = file.path().to_owned();

        serde_yaml::to_writer(&mut file, &config).expect("Failed to write config file");

        let child = Command::new(std::env::current_dir().unwrap().join(CFGSYNC_BIN_PATH))
            .arg(&config_path)
            .current_dir(dir.path())
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .spawn()
            .expect("Failed to start CfgSync process");

        let addr = SocketAddr::from(([127, 0, 0, 1], config.port));
        let server = Self {
            addr,
            child,
            tempdir: dir,
            config_path,
        };

        server
    }
}
