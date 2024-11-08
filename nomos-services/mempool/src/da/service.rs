/// Re-export for OpenAPI
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use crate::backend::Status;
}

// std
use std::fmt::Debug;

// crates
use futures::StreamExt;
use nomos_da_sampling::storage::DaStorageAdapter;
use rand::{RngCore, SeedableRng};
// internal
use crate::backend::MemPool;
use crate::network::NetworkAdapter;
use crate::{MempoolMetrics, MempoolMsg};
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_da_sampling::{
    backend::DaSamplingServiceBackend, network::NetworkAdapter as DaSamplingNetworkAdapter,
    DaSamplingService, DaSamplingServiceMsg,
};
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::{OutboundRelay, Relay},
    state::{NoOperator, NoState},
    ServiceCore, ServiceData, ServiceId,
};
use tracing::error;

pub struct DaMempoolService<N, P, DB, DN, R, SamplingStorage>
where
    N: NetworkAdapter<Key = P::Key>,
    N::Payload: DispersedBlobInfo + Into<P::Item> + Debug + 'static,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    P::BlockId: Debug + 'static,
    DB: DaSamplingServiceBackend<R, BlobId = P::Key> + Send,
    DB::Blob: Debug + 'static,
    DB::BlobId: Debug + 'static,
    DB::Settings: Clone,
    DN: DaSamplingNetworkAdapter,
    SamplingStorage: DaStorageAdapter,
    R: SeedableRng + RngCore,
{
    service_state: ServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    sampling_relay: Relay<DaSamplingService<DB, DN, R, SamplingStorage>>,
    pool: P,
}

impl<N, P, DB, DN, R, DaStorage> ServiceData for DaMempoolService<N, P, DB, DN, R, DaStorage>
where
    N: NetworkAdapter<Key = P::Key>,
    N::Payload: DispersedBlobInfo + Debug + Into<P::Item> + 'static,
    P: MemPool,
    P::Settings: Clone,
    P::Item: Debug + 'static,
    P::Key: Debug + 'static,
    P::BlockId: Debug + 'static,
    DB: DaSamplingServiceBackend<R, BlobId = P::Key> + Send,
    DB::Blob: Debug + 'static,
    DB::BlobId: Debug + 'static,
    DB::Settings: Clone,
    DN: DaSamplingNetworkAdapter,
    DaStorage: DaStorageAdapter,
    R: SeedableRng + RngCore,
{
    const SERVICE_ID: ServiceId = "mempool-da";
    type Settings = DaMempoolSettings<P::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = MempoolMsg<
        <P as MemPool>::BlockId,
        <N as NetworkAdapter>::Payload,
        <P as MemPool>::Item,
        <P as MemPool>::Key,
    >;
}

#[async_trait::async_trait]
impl<N, P, DB, DN, R, DaStorage> ServiceCore for DaMempoolService<N, P, DB, DN, R, DaStorage>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Clone + Debug + Send + Sync + 'static,
    P::BlockId: Send + Debug + 'static,
    N::Payload: DispersedBlobInfo + Into<P::Item> + Clone + Debug + Send + 'static,
    N: NetworkAdapter<Key = P::Key> + Send + Sync + 'static,
    DB: DaSamplingServiceBackend<R, BlobId = P::Key> + Send,
    DB::Blob: Debug + 'static,
    DB::BlobId: Debug + 'static,
    DB::Settings: Clone,
    DN: DaSamplingNetworkAdapter,
    DaStorage: DaStorageAdapter,
    R: SeedableRng + RngCore,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        let sampling_relay = service_state.overwatch_handle.relay();
        let settings = service_state.settings_reader.get_updated_settings();

        Ok(Self {
            service_state,
            network_relay,
            sampling_relay,
            pool: P::new(settings.backend),
        })
    }

    async fn run(mut self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            mut service_state,
            network_relay,
            sampling_relay,
            mut pool,
            ..
        } = self;

        let mut network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let sampling_relay: OutboundRelay<_> = sampling_relay
            .connect()
            .await
            .expect("Relay connection with SamplingService should succeed");

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
                Some((key, item)) = network_items.next() => {
                    sampling_relay.send(DaSamplingServiceMsg::TriggerSampling{blob_id: key.clone()}).await.expect("Sampling trigger message needs to be sent");
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {}", e)
                    });
                    tracing::info!(counter.da_mempool_pending_items = pool.pending_item_count());
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

impl<N, P, DB, DN, R, DaStorage> DaMempoolService<N, P, DB, DN, R, DaStorage>
where
    P: MemPool + Send + 'static,
    P::Settings: Clone + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    P::Item: Clone + Debug + Send + Sync + 'static,
    P::Key: Debug + Send + Sync + 'static,
    P::BlockId: Debug + Send + 'static,
    N::Payload: DispersedBlobInfo + Into<P::Item> + Debug + Clone + Send + 'static,
    N: NetworkAdapter<Key = P::Key> + Send + Sync + 'static,
    DB: DaSamplingServiceBackend<R, BlobId = P::Key> + Send,
    DB::Blob: Debug + 'static,
    DB::BlobId: Debug + 'static,
    DB::Settings: Clone,
    DN: DaSamplingNetworkAdapter,
    DaStorage: DaStorageAdapter,
    R: SeedableRng + RngCore,
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
        message: MempoolMsg<P::BlockId, N::Payload, P::Item, P::Key>,
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
pub struct DaMempoolSettings<B, N> {
    pub backend: B,
    pub network: N,
}
