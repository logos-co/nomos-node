use crate::backend::KMSBackend;
use crate::secure_key::SecuredKey;
use bytes::Bytes;
use either::Either;
use futures::StreamExt;
use log::error;
use overwatch_rs::services::handle::ServiceStateHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::RelayMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::DynError;
use std::fmt::{Debug, Formatter};
use std::future::Future;
use tokio::sync::oneshot;

mod backend;
mod secure_key;

const KMS_TAG: ServiceId = "KMS";

// TODO: Use [`AsyncFnMut`](https://doc.rust-lang.org/stable/std/ops/trait.AsyncFnMut.html#tymethod.async_call_mut) once it is stabilized.
pub type KMSOperator = Box<
    dyn FnMut(&mut dyn SecuredKey) -> Box<dyn Future<Output = Result<(), DynError>>> + Send + Sync,
>;

pub enum KMSMessage<Backend>
where
    Backend: KMSBackend,
{
    Register {
        key: Either<Backend::SupportedKeys, Backend::KeyId>,
        reply_channel: oneshot::Sender<Backend::KeyId>,
    },
    PublicKey {
        key_id: Backend::KeyId,
        reply_channel: oneshot::Sender<Bytes>,
    },
    Sign {
        key_id: Backend::KeyId,
        data: Bytes,
        reply_channel: oneshot::Sender<Bytes>,
    },
    Execute {
        operator: KMSOperator,
    },
}

impl<Backend> Debug for KMSMessage<Backend>
where
    Backend: KMSBackend,
    Backend::KeyId: Debug,
    Backend::SupportedKeys: Debug,
{
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        match self {
            KMSMessage::Register {
                key: Either::Right(key_id),
                ..
            } => {
                write!(f, "KMS-Register {{ KeyId: {key_id:?} }}")
            }
            KMSMessage::Register {
                key: Either::Left(key_type),
                ..
            } => {
                write!(f, "KMS-Register {{ KeyType: {key_type:?} }}")
            }
            KMSMessage::PublicKey { key_id, .. } => {
                write!(f, "KMS-PublicKey {{ KeyId: {key_id:?} }}")
            }
            KMSMessage::Sign { key_id, .. } => {
                write!(f, "KMS-Sign {{ KeyId: {key_id:?} }}")
            }
            KMSMessage::Execute { .. } => {
                write!(f, "KMS-Execute")
            }
        }
    }
}

impl<B: backend::KMSBackend + 'static> RelayMessage for KMSMessage<B> {}

#[derive(Clone)]
pub struct KMSServiceSettings<BackendSettings> {
    backend_settings: BackendSettings,
}

pub struct KMSService<Backend>
where
    Backend: KMSBackend + 'static,
    Backend::KeyId: Debug,
    Backend::SupportedKeys: Debug,
    Backend::Settings: Clone,
{
    backend: Backend,
    service_state: ServiceStateHandle<Self>,
}

impl<Backend> ServiceData for KMSService<Backend>
where
    Backend: KMSBackend + 'static,
    Backend::KeyId: Debug,
    Backend::SupportedKeys: Debug,
    Backend::Settings: Clone,
{
    const SERVICE_ID: ServiceId = KMS_TAG;
    type Settings = KMSServiceSettings<Backend::Settings>;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = KMSMessage<Backend>;
}

#[async_trait::async_trait]
impl<Backend> ServiceCore for KMSService<Backend>
where
    Backend: KMSBackend + Send + 'static,
    Backend::KeyId: Debug + Send,
    Backend::SupportedKeys: Debug + Send,
    Backend::Settings: Clone + Send + Sync,
{
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        let KMSServiceSettings { backend_settings } =
            service_state.settings_reader.get_updated_settings();
        let backend = Backend::new(backend_settings);
        Ok(Self {
            service_state,
            backend,
        })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self {
            mut service_state,
            mut backend,
        } = self;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                Some(msg) = service_state.inbound_relay.recv() => {
                    Self::handle_kms_message(msg, &mut backend).await;
                }
                Some(msg) = lifecycle_stream.next() => {
                    Self::should_stop_service(msg).await;
                }
            }
        }
    }
}

impl<Backend> KMSService<Backend>
where
    Backend: KMSBackend + 'static,
    Backend::KeyId: Debug,
    Backend::SupportedKeys: Debug,
    Backend::Settings: Clone,
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
    async fn handle_kms_message(msg: KMSMessage<Backend>, backend: &mut Backend) {
        match msg {
            KMSMessage::Register { key, reply_channel } => {
                let Ok(key_id) = backend.register(key) else {
                    panic!("A key could not be registered");
                };
                if let Err(_key_id) = reply_channel.send(key_id) {
                    error!("Could not reply key_id for register request");
                }
            }
            KMSMessage::PublicKey {
                key_id,
                reply_channel,
            } => {
                let Ok(pk_bytes) = backend.public_key(key_id) else {
                    panic!("Requested public key for nonexistent KeyId");
                };
                if let Err(_pk_bytes) = reply_channel.send(pk_bytes) {
                    error!("Could not reply public key to request channel");
                }
            }
            KMSMessage::Sign {
                key_id,
                data,
                reply_channel,
            } => {
                let Ok(signature) = backend.sign(key_id, data) else {
                    panic!("Could not sign ")
                };
                if let Err(_signature) = reply_channel.send(signature) {
                    error!("Could not reply public key to request channel");
                }
            }
            KMSMessage::Execute { operator } => {
                backend.execute(operator).await;
            }
        }
    }
}