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
    }, DynError,
};

pub struct Foo;

impl ServiceData for Foo {
    const SERVICE_ID: ServiceId = "Foo";

    type Settings = ();

    type State = NoState<()>;

    type StateOperator = NoOperator<Self::State>;

    type Message = NoMessage;
}


#[async_trait::async_trait]
impl ServiceCore for Foo {
    /// Initialize the service with the given state
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        Ok(Self)
    }

    /// Service main loop
    async fn run(mut self) -> Result<(), DynError> {
        Ok(())
    }
}

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpServer<Foo>>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    http: ServerSettings,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    let settings = Args::parse();
    let graphql = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            http: settings.http,
        },
        None,
    )?;

    // tracing_subscriber::fmt::fmt()
    //     // .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
    //     .with_file(false)
        // .init();

    graphql.wait_finished();
    Ok(())
}
