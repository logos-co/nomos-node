use std::error::Error;
use std::sync::Arc;

use bytes::Bytes;
use clap::Parser;
use nomos_http::backends::HttpBackend;
use nomos_http::bridge::{build_http_bridge, HttpBridgeRunner, HttpBridgeService};
use nomos_http::{
    backends::axum::{AxumBackend, AxumBackendSettings},
    http::*,
};
use overwatch_rs::services::relay::{OutboundRelay, RelayMessage};
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};
use parking_lot::Mutex;
use tokio::sync::oneshot;

async fn handle_count(counter: Arc<Mutex<i32>>, reply_channel: oneshot::Sender<i32>) {
    *counter.lock() += 1;
    let count = *counter.lock();

    if let Err(e) = reply_channel.send(count) {
        tracing::error!("dummy service send error: {e}");
    }
}

fn dummy_graphql_router<B>(
    handle: overwatch_rs::overwatch::handle::OverwatchHandle,
) -> HttpBridgeRunner
where
    B: HttpBackend + Send + Sync + 'static,
    B::Error: Error + Send + Sync + 'static,
{
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

    Box::new(Box::pin(async move {
        // TODO: Graphql supports http GET requests, should nomos support that?
        let (dummy, mut hello_res_rx) =
            build_http_bridge::<DummyGraphqlService, B, _>(handle, HttpMethod::POST, "")
                .await
                .unwrap();

        while let Some(HttpRequest {
            query: _,
            payload,
            res_tx,
        }) = hello_res_rx.recv().await
        {
            let res = match handle_graphql_req(&SCHEMA, payload, dummy.clone()).await {
                Ok(r) => r,
                Err(err) => {
                    tracing::error!(err);
                    err.to_string()
                }
            };

            res_tx.send(Ok(res.into())).await.unwrap();
        }
        Ok(())
    }))
}

async fn handle_graphql_req(
    schema: &async_graphql::Schema<
        DummyGraphql,
        async_graphql::EmptyMutation,
        async_graphql::EmptySubscription,
    >,
    payload: Option<Bytes>,
    dummy: OutboundRelay<DummyGraphqlMsg>,
) -> Result<String, overwatch_rs::DynError> {
    // TODO: Move to the graphql frontend as a helper function?
    let payload = payload.ok_or("empty payload")?;
    let req = async_graphql::http::receive_batch_json(&payload[..])
        .await?
        .into_single()?;

    let (sender, receiver) = oneshot::channel();
    dummy
        .send(DummyGraphqlMsg {
            reply_channel: sender,
        })
        .await
        .unwrap();

    // wait for the dummy service to respond
    receiver.await.unwrap();
    let res = serde_json::to_string(&schema.execute(req).await)?;
    Ok(res)
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
    val: Arc<Mutex<i32>>,
    service_state: ServiceStateHandle<Self>,
}

#[derive(Debug)]
pub struct DummyGraphqlMsg {
    reply_channel: oneshot::Sender<i32>,
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
        Ok(Self {
            service_state,
            val: Arc::new(Mutex::new(0)),
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state: ServiceStateHandle {
                mut inbound_relay, ..
            },
            val,
        } = self;

        // Handle the http request to dummy service.
        while let Some(msg) = inbound_relay.recv().await {
            handle_count(val.clone(), msg.reply_channel).await;
        }

        Ok(())
    }
}

#[derive(overwatch_derive::Services)]
struct Services {
    http: ServiceHandle<HttpService<AxumBackend>>,
    router: ServiceHandle<HttpBridgeService>,
    dummy_graphql: ServiceHandle<DummyGraphqlService>,
}

#[derive(clap::Parser)]
pub struct Args {
    #[clap(flatten)]
    http: AxumBackendSettings,
}

fn main() -> Result<(), overwatch_rs::DynError> {
    tracing_subscriber::fmt::fmt().with_file(false).init();

    let settings = Args::parse();
    let app = OverwatchRunner::<Services>::run(
        ServicesServiceSettings {
            http: nomos_http::http::HttpServiceSettings {
                backend: settings.http,
            },
            router: nomos_http::bridge::HttpBridgeSettings {
                bridges: vec![Arc::new(Box::new(dummy_graphql_router::<AxumBackend>))],
            },
            dummy_graphql: (),
        },
        None,
    )?;

    tracing::info!("overwatch ready");
    app.wait_finished();
    Ok(())
}
