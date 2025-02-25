/// Re-export for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use crate::backend::Status;
}

use std::fmt::Debug;
// std
use std::marker::PhantomData;

// crates
use futures::StreamExt;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::state::ServiceState;
use serde::de::DeserializeOwned;
use serde::Serialize;
use services_utils::overwatch::recovery::operators::RecoveryBackend as RecoveryBackendTrait;
// internal
use crate::backend::RecoverableMempool;
use crate::network::NetworkAdapter as NetworkAdapterTrait;
use crate::tx::settings::TxMempoolSettings;
use crate::{MempoolMetrics, MempoolMsg};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::{relay::OutboundRelay, ServiceCore, ServiceData, ServiceId};
use overwatch_rs::OpaqueServiceStateHandle;
use services_utils::overwatch::{lifecycle, RecoveryOperator};

pub struct TxMempoolService<Pool, NetworkAdapter, RecoveryBackend>
where
    Self: ServiceData,
{
    pool: Pool,
    service_state_handle: OpaqueServiceStateHandle<Self>,
    _phantom: PhantomData<(NetworkAdapter, RecoveryBackend)>,
}

impl<Pool, NetworkAdapter, RecoveryBackend> TxMempoolService<Pool, NetworkAdapter, RecoveryBackend>
where
    Self: ServiceData,
{
    pub const fn new(pool: Pool, service_state_handle: OpaqueServiceStateHandle<Self>) -> Self {
        Self {
            pool,
            service_state_handle,
            _phantom: PhantomData,
        }
    }
}

impl<Pool, NetworkAdapter, RecoveryBackend> ServiceData
    for TxMempoolService<Pool, NetworkAdapter, RecoveryBackend>
where
    Pool: RecoverableMempool,
    Pool::RecoveryState: ServiceState + Serialize + DeserializeOwned,
    NetworkAdapter: NetworkAdapterTrait,
    RecoveryBackend: RecoveryBackendTrait,
{
    const SERVICE_ID: ServiceId = "mempool-cl";
    type Settings = TxMempoolSettings<Pool::Settings, NetworkAdapter::Settings>;
    type State = Pool::RecoveryState;
    type StateOperator = RecoveryOperator<RecoveryBackend>;
    type Message = MempoolMsg<Pool::BlockId, Pool::Item, Pool::Item, Pool::Key>;
}

#[async_trait::async_trait]
impl<Pool, NetworkAdapter, RecoveryBackend> ServiceCore
    for TxMempoolService<Pool, NetworkAdapter, RecoveryBackend>
where
    Pool: RecoverableMempool + Send,
    Pool::RecoveryState: ServiceState + Debug + Serialize + DeserializeOwned + Send + Sync,
    Pool::Settings: Clone + Send + Sync,
    Pool::BlockId: Send + 'static,
    Pool::Key: Send,
    Pool::Item: Clone + Send + 'static,
    NetworkAdapter: NetworkAdapterTrait<Payload = Pool::Item, Key = Pool::Key> + Send,
    NetworkAdapter::Settings: Clone + Send + Sync + 'static,
    RecoveryBackend: RecoveryBackendTrait + Send,
{
    fn init(
        service_state: ServiceStateHandle<Self::Message, Self::Settings, Self::State>,
        init_state: Self::State,
    ) -> Result<Self, overwatch_rs::DynError> {
        tracing::trace!(
            "Initializing TxMempoolService with initial state {:#?}",
            init_state
        );
        let settings = service_state.settings_reader.get_updated_settings();
        let recovered_pool = Pool::recover(settings.pool, init_state);

        Ok(Self::new(recovered_pool, service_state))
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let network_service_relay = self
            .service_state_handle
            .overwatch_handle
            .relay::<NetworkService<_>>()
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        // Queue for network messages
        let mut network_items = NetworkAdapter::new(
            self.service_state_handle
                .settings_reader
                .get_updated_settings()
                .network_adapter,
            network_service_relay.clone(),
        )
        .await
        .payload_stream()
        .await;
        // Queue for lifecycle messages
        let mut lifecycle_stream = self.service_state_handle.lifecycle_handle.message_stream();

        loop {
            tokio::select! {
                // Queue for relay messages
                Some(relay_msg) = self.service_state_handle.inbound_relay.recv() => {
                    self.handle_mempool_message(relay_msg, network_service_relay.clone());
                }
                Some((key, item )) = network_items.next() => {
                    self.pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {e}");
                    });
                    tracing::info!(counter.tx_mempool_pending_items = self.pool.pending_item_count());
                    self.service_state_handle.state_updater.update(self.pool.save());
                }
                Some(lifecycle_msg) = lifecycle_stream.next() =>  {
                    if lifecycle::should_stop_service::<Self>(&lifecycle_msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<Pool, NetworkAdapter, RecoveryBackend> TxMempoolService<Pool, NetworkAdapter, RecoveryBackend>
where
    Pool: RecoverableMempool + Send,
    Pool::RecoveryState: ServiceState + Serialize + DeserializeOwned + Send + Sync,
    Pool::Item: Clone + Send + 'static,
    Pool::Key: Send,
    Pool::BlockId: Send,
    Pool::Settings: Clone + Send + Sync,
    NetworkAdapter: NetworkAdapterTrait<Payload = Pool::Item> + Send,
    NetworkAdapter::Settings: Clone + Send + Sync + 'static,
    RecoveryBackend: RecoveryBackendTrait,
{
    fn handle_mempool_message(
        &mut self,
        message: MempoolMsg<Pool::BlockId, Pool::Item, Pool::Item, Pool::Key>,
        network_relay: OutboundRelay<NetworkMsg<NetworkAdapter::Backend>>,
    ) {
        match message {
            MempoolMsg::Add {
                payload: item,
                key,
                reply_channel,
            } => {
                match self.pool.add_item(key, item.clone()) {
                    Ok(_id) => {
                        // Broadcast the item to the network
                        let settings = self
                            .service_state_handle
                            .settings_reader
                            .get_updated_settings()
                            .network_adapter;
                        self.service_state_handle
                            .state_updater
                            .update(self.pool.save());
                        // move sending to a new task so local operations can complete in the meantime
                        tokio::spawn(async {
                            let adapter = NetworkAdapter::new(settings, network_relay).await;
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
                    .send(self.pool.view(ancestor_hint))
                    .unwrap_or_else(|_| tracing::debug!("could not send back pool view"));
            }
            MempoolMsg::MarkInBlock { ids, block } => {
                self.pool.mark_in_block(&ids, block);
            }
            #[cfg(test)]
            MempoolMsg::BlockItems {
                block,
                reply_channel,
            } => {
                reply_channel
                    .send(self.pool.block_items(block))
                    .unwrap_or_else(|_| tracing::debug!("could not send back block items"));
            }
            MempoolMsg::Prune { ids } => {
                self.pool.prune(&ids);
            }
            MempoolMsg::Metrics { reply_channel } => {
                let metrics = MempoolMetrics {
                    pending_items: self.pool.pending_item_count(),
                    last_item_timestamp: self.pool.last_item_timestamp(),
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
                    .send(self.pool.status(&items))
                    .unwrap_or_else(|_| tracing::debug!("could not send back mempool status"));
            }
        }
    }
}
