// std

// crates
use futures::stream::StreamExt;
use log::error;
// internal
use overwatch_rs::overwatch::handle::OverwatchHandle;
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::relay::NoMessage;
use overwatch_rs::services::state::{NoOperator, NoState};
use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
use overwatch_rs::{DynError, OpaqueServiceStateHandle};

pub struct SystemSig {
    service_state: OpaqueServiceStateHandle<Self>,
}

impl SystemSig {
    async fn should_stop_service(msg: LifecycleMessage) -> bool {
        match msg {
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

    async fn ctrlc_signal_received(overwatch_handle: &OverwatchHandle) {
        overwatch_handle.kill().await
    }
}

impl ServiceData for SystemSig {
    const SERVICE_ID: ServiceId = "SystemSig";
    const SERVICE_RELAY_BUFFER_SIZE: usize = 1;
    type Settings = ();
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State, Self::Settings>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl ServiceCore for SystemSig {
    fn init(
        service_state: OpaqueServiceStateHandle<Self>,
        _init_state: Self::State,
    ) -> Result<Self, DynError> {
        Ok(Self { service_state })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self { service_state } = self;
        let mut ctrlc = async_ctrlc::CtrlC::new()?;
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            tokio::select! {
                _ = &mut ctrlc => {
                    Self::ctrlc_signal_received(&service_state.overwatch_handle).await;
                }
                Some(msg) = lifecycle_stream.next() => {
                    if  Self::should_stop_service(msg).await {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}
