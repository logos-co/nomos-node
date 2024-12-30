pub mod backend;
pub mod network;
pub mod storage;

// std
use std::error::Error;
use std::fmt::{Debug, Formatter};
// crates
use nomos_core::da::blob::Blob;
use nomos_da_network_service::NetworkService;
use nomos_storage::StorageService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::{Relay, RelayMessage};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use serde::{Deserialize, Serialize};
use tokio::sync::oneshot::Sender;
use tokio_stream::StreamExt;
use tracing::error;
use tracing::instrument;
// internal
use backend::VerifierBackend;
use network::NetworkAdapter;
use nomos_tracing::info_with_id;
use storage::DaStorageAdapter;

const DA_VERIFIER_TAG: ServiceId = "DA-Verifier";
pub enum DaVerifierMsg<B, A> {
    AddBlob {
        blob: B,
        reply_channel: Sender<Option<A>>,
    },
}

impl<B: 'static, A: 'static> Debug for DaVerifierMsg<B, A> {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            DaVerifierMsg::AddBlob { .. } => {
                write!(f, "DaVerifierMsg::AddBlob")
            }
        }
    }
}

impl<B: 'static, A: 'static> RelayMessage for DaVerifierMsg<B, A> {}

pub struct DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    Backend::DaBlob: 'static,
    Backend::Error: Error,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    storage_relay: Relay<StorageService<S::Backend>>,
    verifier: Backend,
}

impl<Backend, N, S> DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend + Send + 'static,
    Backend::DaBlob: Debug + Send,
    Backend::Error: Error + Send + Sync,
    Backend::Settings: Clone,
    <Backend::DaBlob as Blob>::BlobId: AsRef<[u8]>,
    N: NetworkAdapter<Blob = Backend::DaBlob> + Send + 'static,
    N::Settings: Clone,
    S: DaStorageAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + 'static,
{
    #[instrument(skip_all)]
    async fn handle_new_blob(
        verifier: &Backend,
        storage_adapter: &S,
        blob: &Backend::DaBlob,
    ) -> Result<(), DynError> {
        if storage_adapter
            .get_blob(blob.id(), blob.column_idx())
            .await?
            .is_some()
        {
            info_with_id!(blob.id().as_ref(), "VerifierBlobExists");
            Ok(())
        } else {
            info_with_id!(blob.id().as_ref(), "VerifierAddBlob");
            verifier.verify(blob)?;
            storage_adapter.add_blob(blob, &()).await?;
            Ok(())
        }
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

impl<Backend, N, S> ServiceData for DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend,
    Backend::Settings: Clone,
    Backend::Error: Error,
    N: NetworkAdapter,
    N::Settings: Clone,
    S: DaStorageAdapter,
    S::Settings: Clone,
{
    const SERVICE_ID: ServiceId = DA_VERIFIER_TAG;
    type Settings = DaVerifierServiceSettings<Backend::Settings, N::Settings, S::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = DaVerifierMsg<Backend::DaBlob, ()>;
}

#[async_trait::async_trait]
impl<Backend, N, S> ServiceCore for DaVerifierService<Backend, N, S>
where
    Backend: VerifierBackend + Send + Sync + 'static,
    Backend::Settings: Clone + Send + Sync + 'static,
    Backend::DaBlob: Debug + Send + Sync + 'static,
    Backend::Error: Error + Send + Sync + 'static,
    <Backend::DaBlob as Blob>::BlobId: AsRef<[u8]>,
    N: NetworkAdapter<Blob = Backend::DaBlob> + Send + Sync + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    S: DaStorageAdapter<Blob = Backend::DaBlob, Attestation = ()> + Send + Sync + 'static,
    S::Settings: Clone + Send + Sync + 'static,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let DaVerifierServiceSettings {
            verifier_settings, ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = service_state.overwatch_handle.relay();
        let storage_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            network_relay,
            storage_relay,
            service_state,
            verifier: Backend::new(verifier_settings),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        // This service will likely have to be modified later on.
        // Most probably the verifier itself need to be constructed/update for every message with
        // an updated list of the available nodes list, as it needs his own index coming from the
        // position of his bls public key landing in the above-mentioned list.
        let Self {
            network_relay,
            storage_relay,
            mut service_state,
            verifier,
        } = self;

        let DaVerifierServiceSettings {
            network_adapter_settings,
            storage_adapter_settings,
            ..
        } = service_state.settings_reader.get_updated_settings();

        let network_relay = network_relay.connect().await?;
        let network_adapter = N::new(network_adapter_settings, network_relay).await;
        let mut blob_stream = network_adapter.blob_stream().await;

        let storage_relay = storage_relay.connect().await?;
        let storage_adapter = S::new(storage_adapter_settings, storage_relay).await;

        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(blob) = blob_stream.next() => {
                    if let Err(err) =  Self::handle_new_blob(&verifier,&storage_adapter, &blob).await {
                        error!("Error handling blob {blob:?} due to {err:?}");
                    }
                }
                Some(msg) = service_state.inbound_relay.recv() => {
                    let DaVerifierMsg::AddBlob { blob, reply_channel } = msg;
                    match Self::handle_new_blob(&verifier, &storage_adapter, &blob).await {
                        Ok(attestation) => {
                            if let Err(err) = reply_channel.send(Some(attestation)) {
                                error!("Error replying attestation {err:?}");
                            }
                        },
                        Err(err) => {
                            error!("Error handling blob {blob:?} due to {err:?}");
                            if let Err(err) = reply_channel.send(None) {
                                error!("Error replying attestation {err:?}");
                            }
                        },
                    };
                }
                Some(msg) = lifecycle_stream.next() => {
                    if Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DaVerifierServiceSettings<BackendSettings, NetworkSettings, StorageSettings> {
    pub verifier_settings: BackendSettings,
    pub network_adapter_settings: NetworkSettings,
    pub storage_adapter_settings: StorageSettings,
}
