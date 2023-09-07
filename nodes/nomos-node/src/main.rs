use nomos_node::{Config, LogArgs, Nomos, NomosServiceSettings, Tx};

mod bridges;

use clap::Parser;
use color_eyre::eyre::{eyre, Result};
use nomos_http::bridge::{HttpBridge, HttpBridgeSettings};

#[cfg(feature = "libp2p")]
use nomos_mempool::network::adapters::libp2p::Libp2pAdapter;
#[cfg(feature = "waku")]
use nomos_mempool::network::adapters::waku::WakuAdapter;
#[cfg(feature = "libp2p")]
use nomos_network::backends::libp2p::Libp2p;
#[cfg(feature = "waku")]
use nomos_network::backends::waku::Waku;
use overwatch_rs::overwatch::*;
use std::sync::Arc;

#[derive(Parser, Debug)]
#[command(author, version, about, long_about = None)]
struct Args {
    /// Path for a yaml-encoded network config file
    config: std::path::PathBuf,
    /// Overrides node key in config file
    #[clap(flatten)]
    log_args: LogArgs,
}

fn main() -> Result<()> {
    let Args { config, log_args } = Args::parse();
    let config =
        serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?.update_log(log_args)?;

    let bridges: Vec<HttpBridge> = vec![
        Arc::new(Box::new(bridges::carnot_info_bridge)),
        Arc::new(Box::new(bridges::mempool_metrics_bridge)),
        Arc::new(Box::new(bridges::network_info_bridge)),
        #[cfg(feature = "waku")]
        Arc::new(Box::new(
            bridges::mempool_add_tx_bridge::<Waku, WakuAdapter<Tx>>,
        )),
        #[cfg(feature = "libp2p")]
        Arc::new(Box::new(
            bridges::mempool_add_tx_bridge::<Libp2p, Libp2pAdapter<Tx>>,
        )),
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
