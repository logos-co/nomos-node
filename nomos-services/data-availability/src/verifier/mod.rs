mod network;

use std::fmt::Debug;
use std::marker::PhantomData;
// std
// crates
use tokio_stream::StreamExt;
use tracing::error;
// internal
use crate::verifier::network::NetworkAdapter;
use nomos_core::da::DaVerifier;
use nomos_network::NetworkService;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::relay::{NoMessage, Relay};
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;

pub struct DaVerifierService<N, V>
where
    N: NetworkAdapter,
    N::Settings: Clone,
    V: DaVerifier,
{
    network_relay: Relay<NetworkService<N::Backend>>,
    service_state: ServiceStateHandle<Self>,
    _verifier: PhantomData<V>,
}

#[derive(Clone)]
pub struct DaVerifierServiceSettings<AdapterSettings> {
    network_adapter_settings: AdapterSettings,
}

impl<N, V> ServiceData for DaVerifierService<N, V>
where
    N: NetworkAdapter,
    N::Settings: Clone,
    V: DaVerifier,
{
    const SERVICE_ID: ServiceId = "DaVerifier";
    type Settings = DaVerifierServiceSettings<N::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl<N, V> ServiceCore for DaVerifierService<N, V>
where
    N: NetworkAdapter<Blob = V::DaBlob, Attestation = V::Attestation> + Send + 'static,
    N::Settings: Clone + Send + Sync + 'static,
    V: DaVerifier + Send + Sync + 'static,
    V::DaBlob: Debug,
    V::Attestation: Debug,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let network_relay = service_state.overwatch_handle.relay();
        Ok(Self {
            network_relay,
            service_state,
            _verifier: Default::default(),
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            network_relay,
            service_state,
            ..
        } = self;
        let DaVerifierServiceSettings {
            network_adapter_settings,
            ..
        } = service_state.settings_reader.get_updated_settings();
        let network_relay = network_relay.connect().await?;
        let adapter = N::new(network_adapter_settings, network_relay).await;
        let mut blob_stream = adapter.blob_stream().await;
        while let Some((blob, reply_channel)) = blob_stream.next().await {
            let sk = get_sk();
            // TODO: Get list of DA nodes
            let pks = &[];
            match V::verify(&blob, sk, pks) {
                Some(attestation) => {
                    if let Err(attestation) = reply_channel.send(attestation) {
                        error!("Error replying attestation {:?}", attestation);
                    }
                    // TODO: Send blob to indexer service
                }
                _ => {
                    error!("Received unverified blob {:?}", blob);
                }
            }
        }
        Ok(())
    }
}

fn get_sk<Sk>() -> &'static Sk {
    todo!("Sk retrieval for da verifier is not yet implemented")
}
