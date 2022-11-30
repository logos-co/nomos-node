use metrics_derive::MetricsData;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};

#[repr(transparent)]
pub struct Metrics {
    inbound_relay: InboundRelay<MetricsMessage>,
}

#[derive(Debug, Clone, PartialEq, Eq, async_graphql::SimpleObject)]
pub struct FooMetic {
    pub foo: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, MetricsData)]
pub enum MetricData {
    Foo(FooMetic),
}

#[derive(Debug)]
pub enum MetricsMessage {
    Load {
        service_id: ServiceId,
        tx: tokio::sync::oneshot::Sender<Option<MetricData>>,
    },
    Update {
        service_id: ServiceId,
        data: MetricData,
    },
}

impl RelayMessage for MetricsMessage {}

impl ServiceData for Metrics {
    const SERVICE_ID: ServiceId = "Metrics";
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MetricsMessage;
}

#[async_trait::async_trait]
impl ServiceCore for Metrics {
    fn init(service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            inbound_relay: service_state.inbound_relay,
        }
    }

    async fn run(self) {
        let Self { mut inbound_relay } = self;

        let backend = MetricsBackend::default();

        // thread for handling update metrics
        tokio::spawn(async move {
            loop {
                tokio::select! {
                    msg = inbound_relay.recv() => {
                        if let Some(msg) = msg {
                            match msg {
                                MetricsMessage::Load { service_id, tx } => {
                                    let map = backend.stack.lock();
                                    if let Err(e) = tx.send(map.get(&service_id).cloned()) {
                                        tracing::error!("Failed to send metric data for service: {service_id}. data: {:?}", e);
                                    }
                                },
                                MetricsMessage::Update { service_id, data } => {
                                    let mut map = backend.stack.lock();
                                    match map.entry(service_id) {
                                        std::collections::hash_map::Entry::Occupied(mut val) => *val.get_mut() = data,
                                        std::collections::hash_map::Entry::Vacant(val) => {
                                            val.insert(data);
                                        },
                                    }
                                },
                            }
                        }
                    }
                }
            }
        });
    }
}
