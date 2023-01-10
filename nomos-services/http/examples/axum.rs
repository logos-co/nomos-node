use std::sync::Arc;

use clap::Parser;
use nomos_http::{
    backends::{
        axum::{AxumBackend, AxumBackendSettings},
        HttpBackend,
    },
    *,
};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{NoMessage, Relay},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};
use parking_lot::Mutex;
use tokio::sync::mpsc::channel;

#[derive(Debug, Clone)]
pub struct Foo;

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpService<AxumBackend>>,
    dummy: ServiceHandle<DummyService<AxumBackend>>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    http: AxumBackendSettings,
}

fn main() -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    tracing_subscriber::fmt::fmt().with_file(false).init();

    let settings = Args::parse();
    let app = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            http: nomos_http::Config {
                backend: settings.http,
            },
            dummy: (),
        },
        None,
    )?;

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}

pub struct DummyService<H: HttpBackend> {
    counter: Arc<Mutex<i32>>,
    http_relay: Relay<HttpService<H>>,
}

impl<H: HttpBackend> ServiceData for DummyService<H> {
    const SERVICE_ID: ServiceId = "Dummy";
    type Settings = ();
    type State = NoState<()>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<H> ServiceCore for DummyService<H>
where
    H: HttpBackend<Response = String>,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let http_relay: Relay<HttpService<H>> = service_state.overwatch_handle.relay();
        Ok(Self {
            counter: Default::default(),
            http_relay,
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let http = self.http_relay.connect().await.map_err(|e| {
            tracing::error!(err = ?e, "http relay connect error");
            e
        })?;

        let (hello_req_tx, mut hello_res_rx) = channel(1);

        // Register on http service to receive GET requests.
        // Once registered, the dummy endpoint will be accessable at `http://{addr}/dummy/`.
        http.send(HttpMsg::add_get_handler(Self::SERVICE_ID, "", hello_req_tx))
            .await
            .expect("send message to http service");

        // Imitating shared state that other services might have.
        let counter = self.counter.clone();

        // Handle the http request to dummy service.
        while let Some(req) = hello_res_rx.recv().await {
            handle_hello::<H>(counter.clone(), req).await;
        }

        Ok(())
    }
}

async fn handle_hello<H: HttpBackend>(
    counter: Arc<Mutex<i32>>,
    req: HttpRequest<H::Request, String>,
) {
    *counter.clone().lock() += 1;
    let count = *counter.clone().lock();

    if let Err(e) = req.res_tx.send(format!("hello count: {}", count)).await {
        tracing::error!("dummy service send error: {e}");
    }
}
