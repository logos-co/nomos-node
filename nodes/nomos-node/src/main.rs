use nomos_node::{Config, Nomos, NomosServiceSettings};

mod bridges;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_http::bridge::{HttpBridge, HttpBridgeSettings};

use overwatch_rs::overwatch::*;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
}

fn main() -> Result<()> {
    let Args { config } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?;
    let bridges: Vec<HttpBridge> = vec![
        Arc::new(Box::new(bridges::carnot_info_bridge)),
        Arc::new(Box::new(bridges::mempool_metrics_bridge)),
        Arc::new(Box::new(bridges::network_info_bridge)),
        #[cfg(feature = "waku")]
        Arc::new(Box::new(bridges::mempool_add_tx_bridge)),
        #[cfg(feature = "waku")]
        Arc::new(Box::new(bridges::waku_add_conn_bridge)),
    ];
    let app = OverwatchRunner::<Nomos>::run(
        NomosServiceSettings {
            network: config.network,
            logging: config.log,
            http: config.http,
            mockpool: (),
            consensus: config.consensus,
            bridges: HttpBridgeSettings { bridges },
            #[cfg(feature = "metrics")]
            metrics: config.metrics,
        },
        None,
    )
    .map_err(|e| eyre!("Error encountered: {}", e))?;
    app.wait_finished();
    Ok(())
}
