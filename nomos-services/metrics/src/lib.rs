use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{InboundRelay, RelayMessage},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use parking_lot::Mutex;
use serde::{de::DeserializeOwned, Serialize};
use std::{collections::HashMap, sync::Arc};

pub trait MetricData:
    core::fmt::Debug
    + Clone
    + Serialize
    + DeserializeOwned
    + async_graphql::OutputType
    + Send
    + Sync
    + 'static
{
}

#[repr(transparent)]
pub struct Metrics<E: MetricData> {
    inbound_relay: InboundRelay<MetricsMessage<E>>,
}

#[derive(Debug, Clone)]
#[repr(transparent)]
pub struct MetricsBackend<E> {
    stack: Arc<Mutex<HashMap<ServiceId, E>>>,
}

impl<E> Default for MetricsBackend<E> {
    fn default() -> Self {
        Self {
            stack: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

#[async_graphql::Object]
impl<E: MetricData> MetricsBackend<E> {
    pub async fn load(&self, name: String) -> Option<E> {
        let map = self.stack.lock();
        map.get(name.as_str()).cloned()
    }
}

#[derive(Debug)]
pub enum MetricsMessage<E: MetricData> {
    Load {
        service_id: ServiceId,
        tx: tokio::sync::oneshot::Sender<Option<E>>,
    },
    Update {
        service_id: ServiceId,
        data: E,
    },
}

impl<E> RelayMessage for MetricsMessage<E> where E: MetricData {}

impl<E: MetricData> ServiceData for Metrics<E> {
    const SERVICE_ID: ServiceId = "Metrics";
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MetricsMessage<E>;
}

#[async_trait::async_trait]
impl<E: MetricData> ServiceCore for Metrics<E> {
    fn init(service_state: ServiceStateHandle<Self>) -> Self {
        Self {
            inbound_relay: service_state.inbound_relay,
        }
    }

    async fn run(self) {
        let Self { mut inbound_relay } = self;

        let backend = MetricsBackend::<E>::default();

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
