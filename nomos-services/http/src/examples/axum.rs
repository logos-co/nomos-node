use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
};

use clap::Parser;
use nomos_http::*;
use overwatch_rs::{
    overwatch::OverwatchRunner,
    services::{
        handle::{ServiceHandle, ServiceStateHandle},
        relay::{NoMessage, Relay},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};

#[derive(Debug, Clone)]
pub struct Foo;

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpServer>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    http: ServerSettings,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = Args::parse();
    let app = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            http: settings.http,
        },
        None,
    )?;

    tracing_subscriber::fmt::fmt()
        //.with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}
