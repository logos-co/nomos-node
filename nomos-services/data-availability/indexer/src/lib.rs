// std
use std::fmt::{Debug, Formatter};
use std::marker::PhantomData;
// crates
use futures::StreamExt;
use nomos_core::block::Block;
use nomos_core::da::vid::VID;
use overwatch_rs::services::relay::Relay;
use serde::{Deserialize, Serialize};
use tokio::sync::broadcast;
use tracing::error;
// internal
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::{
    services::{
        handle::ServiceStateHandle,
        relay::RelayMessage,
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};

pub enum DaIndexMsg<V> {
    // TODO: The certificate receival most likely won't happen by sending a new VIDs
    // to the DAIndexService. The service should subscribe to the event about new VIDs
    // observerd/added to the block.
    ReceiveVID { vid: V },
    GetRange {},
}

impl<V: VID + 'static> Debug for DaIndexMsg<V> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaIndexMsg::ReceiveVID { .. } => {
                write!(f, "DaIndexMsg::ReceiveVID")
            }
            DaIndexMsg::GetRange { .. } => {
                write!(f, "DaIndexMsg::Get")
            }
        }
    }
}

impl<V: VID + 'static> RelayMessage for DaIndexMsg<V> {}

pub struct DataIndexerService<Tx, V>
where
    Tx: Clone + Eq + std::hash::Hash + 'static,
    V: VID + Clone + Eq + std::hash::Hash + 'static,
{
    service_state: ServiceStateHandle<Self>,
    incoming_blocks: broadcast::Receiver<Block<Tx, V>>,
    _tx: PhantomData<Tx>,
    _vid: PhantomData<V>,
}

impl<Tx, V> DataIndexerService<Tx, V>
where
    Tx: Clone + Eq + std::hash::Hash,
    V: VID + Clone + Eq + std::hash::Hash,
{
    async fn handle_new_block_msg(block: Block<Tx, V>) -> Result<(), DynError> {
        todo!()
    }

    async fn should_stop_service(message: LifecycleMessage) -> bool {
        match message {
            LifecycleMessage::Shutdown(sender) => {
                if sender.send(()).is_err() {
                    error!(
                        "Error sending successful shutdown signal from service {}",
                        Self::SERVICE_ID
                    );
                }
                true
            }
            LifecycleMessage::Kill => true,
        }
    }
}

impl<Tx, V> ServiceData for DataIndexerService<Tx, V>
where
    Tx: Clone + Eq + std::hash::Hash + 'static,
    V: VID + Clone + Eq + std::hash::Hash + 'static,
{
    const SERVICE_ID: ServiceId = "DAIndexer";
    type Settings = Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaIndexMsg<V>;
}

#[async_trait::async_trait]
impl<Tx, V> ServiceCore for DataIndexerService<Tx, V>
where
    Tx: Clone + Eq + std::hash::Hash + Send + Sync,
    V: VID + Clone + Eq + std::hash::Hash + Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let settings = service_state.settings_reader.get_updated_settings();
        let consensus_relay: Relay<CryptarchiaConsensus> = service_state.overwatch_handle.relay();
        incoming_blocks = todo!();

        Ok(Self {
            service_state,
            incoming_blocks,
            _tx: Default::default(),
            _vid: Default::default(),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state, ..
        } = self;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    todo!()
                }
                Some(msg) = lifecycle_stream.next() => {
                    todo!()
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct Settings {}

