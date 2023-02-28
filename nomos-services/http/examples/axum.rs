use std::error::Error;
use std::sync::Arc;

use clap::Parser;
use nomos_http::backends::HttpBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner, HttpBridgeService};
use nomos_http::{
    backends::axum::{AxumBackend, AxumBackendSettings},
    http::*,
};
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};
use parking_lot::Mutex;
use tokio::sync::oneshot;

pub struct DummyService {
    counter: Arc<Mutex<i32>>,
    service_state: ServiceStateHandle<Self>,
}

#[derive(Debug)]
pub struct DummyMsg {
    reply_channel: oneshot::Sender<i32>,
}

impl RelayMessage for DummyMsg {}

impl ServiceData for DummyService {
    const SERVICE_ID: ServiceId = "Dummy";
    type Settings = ();
    type State = NoState<()>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DummyMsg;
}

#[async_trait::async_trait]
impl ServiceCore for DummyService {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self {
            counter: Default::default(),
            service_state,
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            counter,
            service_state: ServiceStateHandle {
                mut inbound_relay, ..
            },
        } = self;

        // Handle the http request to dummy service.
        while let Some(msg) = inbound_relay.recv().await {
            handle_hello(counter.clone(), msg.reply_channel).await;
        }

        Ok(())
    }
}

async fn handle_hello(counter: Arc<Mutex<i32>>, reply_channel: oneshot::Sender<i32>) {
    *counter.lock() += 1;
    let count = *counter.lock();

    if let Err(e) = reply_channel.send(count) {
        tracing::error!("dummy service send error: {e}");
    }
}

fn dummy_router<B>(handle: overwatch_rs::overwatch::handle::OverwatchHandle) -> HttpBridgeRunner
where
    B: HttpBackend + Send + Sync + 'static,
    B::Error: Error + Send + Sync + 'static,
{
    Box::new(Box::pin(async move {
        let (dummy, mut hello_res_rx) =
            build_http_bridge::<DummyService, B, _>(handle, HttpMethod::GET, "")
                .await
                .unwrap();

        while let Some(HttpRequest { res_tx, .. }) = hello_res_rx.recv().await {
            let (sender, receiver) = oneshot::channel();
            dummy
                .send(DummyMsg {
                    reply_channel: sender,
                })
                .await
                .unwrap();
            let value = receiver.await.unwrap();
            res_tx
                .send(Ok(format!("Hello, world! {value}").into()))
                .await
                .unwrap();
        }
        Ok(())
    }))
}

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpService<AxumBackend>>,
    router: ServiceHandle<HttpBridgeService>,
    dummy: ServiceHandle<DummyService>,
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
            http: nomos_http::http::HttpServiceSettings {
                backend: settings.http,
            },
            router: nomos_http::bridge::HttpBridgeSettings {
                bridges: vec![Arc::new(Box::new(dummy_router::<AxumBackend>))],
            },
            dummy: (),
        },
        None,
    )?;

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}
