use std::{path::PathBuf, process};

use cfgsync::server::{cfgsync_app, CfgSyncConfig};
use clap::Parser;

#[derive(Parser, Debug)]
#[command(about = "CfgSync")]
struct Args {
    config: PathBuf,
}

#[tokio::main]
async fn main() {
    let cli = Args::parse();

    let config = CfgSyncConfig::load_from_file(&cli.config).unwrap_or_else(|err| {
        eprintln!("{}", err);
        process::exit(1);
    });

    let port = config.port;
    let app = cfgsync_app(config.into());

    println!("Server running on http://0.0.0.0:{}", port);
    axum::Server::bind(&format!("0.0.0.0:{}", port).parse().unwrap())
        .serve(app.into_make_service())
        .await
        .unwrap();
}
