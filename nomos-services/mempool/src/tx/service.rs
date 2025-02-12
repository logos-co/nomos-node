/// Re-export for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use crate::backend::Status;
}

// std
use std::fmt::Debug;
use std::marker::PhantomData;
use std::path::PathBuf;

// crates
use futures::StreamExt;
use overwatch_rs::services::state::ServiceState;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use thiserror::Error;
use tracing::error;
use utils::overwatch::recovery::backends::FileBackendSettings;
use utils::overwatch::{JsonFileBackend, RecoveryOperator};
// internal
use crate::backend::{MemPool, RecoverableMempool};
use crate::network::NetworkAdapter;
use crate::{MempoolMetrics, MempoolMsg};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay},
    ServiceCore, ServiceData, ServiceId,
};

#[derive(Error, Debug)]
pub enum StateError {}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MempoolState<NS, PS, P> {
    // `pool` is an option so that we don't need it to implement `Mempool`
    // when implementing `ServiceState` for this type, as otherwise we would need to call `P::new(settings)`.
    pool: Option<P>,
    _phantom: PhantomData<(NS, PS)>,
}

// We need to have three generics instead of two as otherwise we would need `P` to implement `Mempool`
// to access its `Settings` associated type, which would defeat declaring a new type for the recovery state.
impl<NS, PS, P> From<P> for MempoolState<NS, PS, P> {
    fn from(value: P) -> Self {
        Self {
            pool: Some(value),
            _phantom: PhantomData,
        }
    }
}

impl<NS, PS, P> ServiceState for MempoolState<NS, PS, P> {
    type Error = StateError;
    type Settings = TxMempoolSettings<PS, NS>;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self {
            pool: None,
            _phantom: PhantomData,
        })
    }
}

pub struct TxMempoolService<N, P>
where
    N: NetworkAdapter<Payload = P::Item, Key = P::Key>,
    N::Settings: Send,
    P: RecoverableMempool,
    P::RecoveryState: Clone + Send + Sync + Serialize + DeserializeOwned,
    P::Settings: Send + Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    P::BlockId: Debug + 'static,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    pool: P,
}

impl<N, P> ServiceData for TxMempoolService<N, P>
where
    N: NetworkAdapter<Payload = P::Item, Key = P::Key>,
    N::Settings: Send,
    P: RecoverableMempool,
    P::RecoveryState: Clone + Send + Sync + Serialize + DeserializeOwned,
    P::Settings: Clone + Send,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    P::BlockId: Debug + 'static,
{
    const SERVICE_ID: ServiceId = "mempool-cl";
    type Settings = TxMempoolSettings<P::Settings, N::Settings>;
    type State = MempoolState<N::Settings, P::Settings, P::RecoveryState>;
    type StateOperator = RecoveryOperator<JsonFileBackend<Self::State, Self::Settings>>;
    type Message = MempoolMsg<
        <P as MemPool>::BlockId,
        <P as MemPool>::Item,
        <P as MemPool>::Item,
        <P as MemPool>::Key,
    >;
}

#[async_trait::async_trait]
impl<N, P> ServiceCore for TxMempoolService<N, P>
where
    P: RecoverableMempool + Send + 'static,
    P::RecoveryState: Clone + Send + Sync + Serialize + DeserializeOwned,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    P::BlockId: Send + Debug + 'static,
    N: NetworkAdapter<Payload = P::Item, Key = P::Key> + Send + Sync + 'static,
{
    fn init(
        service_state: ServiceStateHandle<Self>,
        init_state: Self::State,
    ) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();

        let recovered_pool = match init_state.pool {
            Some(p) => P::recover(settings.backend, p),
            None => P::new(settings.backend),
        };

        Ok(Self {
            service_state,
            network_relay,
            pool: recovered_pool,
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            network_relay,
            mut pool,
            ..
        } = self;

        let mut network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let adapter = N::new(
            service_state.settings_reader.get_updated_settings().network,
            network_relay.clone(),
        );
        let adapter = adapter.await;

        let mut network_items = adapter.payload_stream().await;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();

        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    Self::handle_mempool_message(msg, &mut pool, &mut network_relay, &mut service_state);
                }
                Some((key, item )) = network_items.next() => {
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {e}");
                    });
                    service_state.state_updater.update(pool.save().into());
                    tracing::info!(counter.tx_mempool_pending_items = pool.pending_item_count());
                }
                Some(msg) = lifecycle_stream.next() =>  {
                    if Self::should_stop_service(msg) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<N, P> TxMempoolService<N, P>
where
    P: RecoverableMempool + Send + 'static,
    P::RecoveryState: Clone + Send + Sync + Serialize + DeserializeOwned,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    P::BlockId: Debug + Send + 'static,
    N: NetworkAdapter<Payload = P::Item, Key = P::Key> + Send + Sync + 'static,
{
    fn should_stop_service(message: LifecycleMessage) -> bool {
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

    fn handle_mempool_message(
        message: MempoolMsg<P::BlockId, P::Item, P::Item, P::Key>,
        pool: &mut P,
        network_relay: &mut OutboundRelay<NetworkMsg<N::Backend>>,
        service_state: &mut ServiceStateHandle<Self>,
    ) {
        match message {
            MempoolMsg::Add {
                payload: item,
                key,
                reply_channel,
            } => {
                match pool.add_item(key, item.clone()) {
                    Ok(_id) => {
                        // Broadcast the item to the network
                        let net = network_relay.clone();
                        let settings = service_state.settings_reader.get_updated_settings().network;
                        service_state.state_updater.update(pool.save().into());
                        // move sending to a new task so local operations can complete in the meantime
                        tokio::spawn(async move {
                            let adapter = N::new(settings, net).await;
                            adapter.send(item).await;
                        });
                        if let Err(e) = reply_channel.send(Ok(())) {
                            tracing::debug!("Failed to send reply to AddTx: {:?}", e);
                        }
                    }
                    Err(e) => {
                        tracing::debug!("could not add tx to the pool due to: {}", e);
                    }
                }
            }
            MempoolMsg::View {
                ancestor_hint,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.view(ancestor_hint))
                    .unwrap_or_else(|_| tracing::debug!("could not send back pool view"));
            }
            MempoolMsg::MarkInBlock { ids, block } => {
                pool.mark_in_block(&ids, block);
            }
            #[cfg(test)]
            MempoolMsg::BlockItems {
                block,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.block_items(block))
                    .unwrap_or_else(|_| tracing::debug!("could not send back block items"));
            }
            MempoolMsg::Prune { ids } => {
                pool.prune(&ids);
                service_state.state_updater.update(pool.save().into());
            }
            MempoolMsg::Metrics { reply_channel } => {
                let metrics = MempoolMetrics {
                    pending_items: pool.pending_item_count(),
                    last_item_timestamp: pool.last_item_timestamp(),
                };
                reply_channel
                    .send(metrics)
                    .unwrap_or_else(|_| tracing::debug!("could not send back mempool metrics"));
            }
            MempoolMsg::Status {
                items,
                reply_channel,
            } => {
                reply_channel
                    .send(pool.status(&items))
                    .unwrap_or_else(|_| tracing::debug!("could not send back mempool status"));
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct TxMempoolSettings<B, N> {
    pub backend: B,
    pub network: N,
    pub recovery_file_path: PathBuf,
}

impl<B, N> FileBackendSettings for TxMempoolSettings<B, N> {
    fn recovery_file(&self) -> &PathBuf {
        &self.recovery_file_path
    }
}
