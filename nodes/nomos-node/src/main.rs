use nomos_node::{
    Config, ConsensusArgs, HttpArgs, LogArgs, NetworkArgs, Nomos, NomosServiceSettings,
    OverlayArgs, Tx,
};

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
    /// Overrides log config.
    #[clap(flatten)]
    log_args: LogArgs,
    /// Overrides network config.
    #[clap(flatten)]
    network_args: NetworkArgs,
    /// Overrides http config.
    #[clap(flatten)]
    http_args: HttpArgs,
    /// Overrides consensus config.
    #[clap(flatten)]
    consensus_args: ConsensusArgs,
    /// Overrides overlay config.
    #[clap(flatten)]
    overlay_args: OverlayArgs,
}

fn main() -> Result<()> {
    let Args {
        config,
        log_args,
        http_args,
        network_args,
        consensus_args,
        overlay_args,
    } = Args::parse();
    let config = serde_yaml::from_reader::<_, Config>(std::fs::File::open(config)?)?
        .update_log(log_args)?
        .update_http(http_args)?
        .update_consensus(consensus_args)?
        .update_overlay(overlay_args)?
        .update_network(network_args)?;

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
