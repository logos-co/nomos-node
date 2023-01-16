use std::error::Error;
use std::sync::Arc;

use clap::Parser;
use nomos_http::backends::HttpBackend;
use nomos_http::bridge::{
    build_graphql_bridge, build_http_bridge, HttpBridgeRunner, HttpBridgeService,
};
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
                .send(format!("Hello, world! {}", value).into())
                .await
                .unwrap();
        }
        Ok(())
    }))
}

fn dummy_graphql_router<B>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner
where
    B: HttpBackend + Send + Sync + 'static,
    B::Error: Error + Send + Sync + 'static,
{
    Box::new(Box::pin(async move {
        let (dummy, mut hello_res_rx) =
            build_graphql_bridge::<DummyGraphqlService, B, _>(handle, "graphql")
                .await
                .unwrap();

        while let Some(GraphqlRequest { res_tx, req }) = hello_res_rx.recv().await {
            let (sender, receiver) = oneshot::channel();
            dummy
                .send(DummyGraphqlMsg {
                    req,
                    reply_channel: sender,
                })
                .await
                .unwrap();
            let res = receiver.await.unwrap();
            res_tx.send(res).await.unwrap();
        }
        Ok(())
    }))
}

#[derive(Debug, Clone, Default)]
pub struct DummyGraphql {
    val: Arc<Mutex<i32>>,
}

#[async_graphql::Object]
impl DummyGraphql {
    async fn val(&self) -> i32 {
        let mut val = self.val.lock();
        *val += 1;
        *val
    }
}

pub struct DummyGraphqlService {
    service_state: ServiceStateHandle<Self>,
}

#[derive(Debug)]
pub struct DummyGraphqlMsg {
    req: async_graphql::Request,
    reply_channel: oneshot::Sender<async_graphql::Response>,
}

impl RelayMessage for DummyGraphqlMsg {}

impl ServiceData for DummyGraphqlService {
    const SERVICE_ID: ServiceId = "DummyGraphqlService";
    type Settings = ();
    type State = NoState<()>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DummyGraphqlMsg;
}

#[async_trait::async_trait]
impl ServiceCore for DummyGraphqlService {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        Ok(Self { service_state })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state: ServiceStateHandle {
                mut inbound_relay, ..
            },
        } = self;

        static SCHEMA: once_cell::sync::Lazy<
            async_graphql::Schema<
                DummyGraphql,
                async_graphql::EmptyMutation,
                async_graphql::EmptySubscription,
            >,
        > = once_cell::sync::Lazy::new(|| {
            async_graphql::Schema::build(
                DummyGraphql::default(),
                async_graphql::EmptyMutation,
                async_graphql::EmptySubscription,
            )
            .finish()
        });

        // Handle the http request to dummy service.
        while let Some(msg) = inbound_relay.recv().await {
            let res = SCHEMA.execute(msg.req).await;
            msg.reply_channel.send(res).unwrap();
        }

        Ok(())
    }
}

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpService<AxumBackend>>,
    router: ServiceHandle<HttpBridgeService>,
    dummy: ServiceHandle<DummyService>,
    dummy_graphql: ServiceHandle<DummyGraphqlService>,
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
            http: nomos_http::http::Config {
                backend: settings.http,
            },
            router: nomos_http::bridge::HttpBridgeSettings {
                runners: vec![
                    Arc::new(Box::new(dummy_router::<AxumBackend>)),
                    Arc::new(Box::new(dummy_graphql_router::<AxumBackend>)),
                ],
            },
            dummy: (),
            dummy_graphql: (),
        },
        None,
    )?;

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}
