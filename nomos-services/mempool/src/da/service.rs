/// Re-export for `OpenAPI`
#[cfg(feature = "openapi")]
pub mod openapi {
    pub use crate::backend::Status;
}

use std::fmt::Debug;

use futures::StreamExt;
use nomos_core::da::blob::info::DispersedBlobInfo;
use nomos_da_sampling::{
    backend::DaSamplingServiceBackend, network::NetworkAdapter as DaSamplingNetworkAdapter,
    storage::DaStorageAdapter, DaSamplingService, DaSamplingServiceMsg,
};
use nomos_da_verifier::backend::VerifierBackend;
use nomos_network::{NetworkMsg, NetworkService};
use overwatch_rs::{
    services::{
        relay::{OutboundRelay, Relay},
        state::{NoOperator, NoState},
        ServiceCore, ServiceData, ServiceId,
    },
    OpaqueServiceStateHandle,
};
use rand::{RngCore, SeedableRng};
use services_utils::overwatch::lifecycle;

use crate::{backend::MemPool, network::NetworkAdapter, MempoolMetrics, MempoolMsg};

pub struct DaMempoolService<
    N,
    P,
    DB,
    DN,
    R,
    SamplingStorage,
    DaVerifierBackend,
    DaVerifierNetwork,
    DaVerifierStorage,
> where
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
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
    DaVerifierBackend: VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
{
    service_state: OpaqueServiceStateHandle<Self>,
    network_relay: Relay<NetworkService<N::Backend>>,
    sampling_relay: Relay<
        DaSamplingService<
            DB,
            DN,
            R,
            SamplingStorage,
            DaVerifierBackend,
            DaVerifierNetwork,
            DaVerifierStorage,
        >,
    >,
    pool: P,
}

impl<N, P, DB, DN, R, DaStorage, DaVerifierBackend, DaVerifierNetwork, DaVerifierStorage>
    ServiceData
    for DaMempoolService<
        N,
        P,
        DB,
        DN,
        R,
        DaStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
    >
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
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter,
    DaVerifierBackend: VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
{
    const SERVICE_ID: ServiceId = "mempool-da";
    type Settings = DaMempoolSettings<P::Settings, N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
    type Message = MempoolMsg<
        <P as MemPool>::BlockId,
        <N as NetworkAdapter>::Payload,
        <P as MemPool>::Item,
        <P as MemPool>::Key,
    >;
}

#[async_trait::async_trait]
impl<N, P, DB, DN, R, DaStorage, DaVerifierBackend, DaVerifierNetwork, DaVerifierStorage>
    ServiceCore
    for DaMempoolService<
        N,
        P,
        DB,
        DN,
        R,
        DaStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
    >
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
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter + Send + Sync,
    DaVerifierBackend: nomos_da_verifier::backend::VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
{
    fn init(
        service_state: OpaqueServiceStateHandle<Self>,
        _init_state: Self::State,
    ) -> Result<Self, overwatch_rs::DynError> {
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

        let network_relay: OutboundRelay<_> = network_relay
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
                    Self::handle_mempool_message(msg, &mut pool, &network_relay, &service_state);
                }
                Some((key, item)) = network_items.next() => {
                    sampling_relay.send(DaSamplingServiceMsg::TriggerSampling{blob_id: key.clone()}).await.expect("Sampling trigger message needs to be sent");
                    pool.add_item(key, item).unwrap_or_else(|e| {
                        tracing::debug!("could not add item to the pool due to: {e}");
                    });
                    tracing::info!(counter.da_mempool_pending_items = pool.pending_item_count());
                }
                Some(msg) = lifecycle_stream.next() =>  {
                    if lifecycle::should_stop_service::<Self>(&msg) {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

impl<N, P, DB, DN, R, DaStorage, DaVerifierBackend, DaVerifierNetwork, DaVerifierStorage>
    DaMempoolService<
        N,
        P,
        DB,
        DN,
        R,
        DaStorage,
        DaVerifierBackend,
        DaVerifierNetwork,
        DaVerifierStorage,
    >
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
    DaVerifierStorage: nomos_da_verifier::storage::DaStorageAdapter + Send + Sync,
    DaVerifierBackend: VerifierBackend + Send + 'static,
    DaVerifierBackend::Settings: Clone,
    DaVerifierNetwork: nomos_da_verifier::network::NetworkAdapter,
    DaVerifierNetwork::Settings: Clone,
{
    fn handle_mempool_message(
        message: MempoolMsg<P::BlockId, N::Payload, P::Item, P::Key>,
        pool: &mut P,
        network_relay: &OutboundRelay<NetworkMsg<N::Backend>>,
        service_state: &OpaqueServiceStateHandle<Self>,
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
                        // move sending to a new task so local operations can complete in the
                        // meantime
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
