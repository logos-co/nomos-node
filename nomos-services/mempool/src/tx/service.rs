/// Re-export for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use crate::backend::Status;
}

// std
use std::fmt::Debug;

// crates
use futures::StreamExt;
// internal
use crate::backend::MemPool;
use crate::network::NetworkAdapter;
use crate::{MempoolMetrics, MempoolMsg};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use tracing::error;

pub struct TxMempoolService<N, P>
where
    N: NetworkAdapter<Payload = P::Item, Key = P::Key>,
    P: MemPool,
    P::Settings: Clone,
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
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    P::BlockId: Debug + 'static,
{
    const SERVICE_ID: ServiceId = "mempool-cl";
    type Settings = TxMempoolSettings<P::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
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
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    P::BlockId: Send + Debug + 'static,
    N: NetworkAdapter<Payload = P::Item, Key = P::Key> + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();

        Ok(Self {
            service_state,
            network_relay,
            pool: P::new(settings.backend),
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
                    Self::handle_mempool_message(msg, &mut pool, &mut network_relay, &mut service_state).await;
                }
                Some((key, item )) = network_items.next() => {
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {}", e)
                    });
                    tracing::info!(counter.tx_mempool_pending_items = pool.pending_item_count());
                }
                Some(msg) = lifecycle_stream.next() =>  {
                    if Self::should_stop_service(msg).await {
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
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    P::BlockId: Debug + Send + 'static,
    N: NetworkAdapter<Payload = P::Item, Key = P::Key> + Send + Sync + 'static,
{
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

    async fn handle_mempool_message(
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
}
