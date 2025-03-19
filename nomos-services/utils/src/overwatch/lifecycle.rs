use std::fmt::Display;

use overwatch::services::{life_cycle::LifecycleMessage, ServiceId};
use tracing::{debug, error};

/// Handles the shutdown signal from `Overwatch`
pub fn should_stop_service<S: ServiceId<RuntimeServiceId>, RuntimeServiceId>(
    msg: &LifecycleMessage,
) -> bool
where
    RuntimeServiceId: Display,
{
    match msg {
        LifecycleMessage::Shutdown(sender) => {
            if sender.send(()).is_err() {
                error!(
                    "Error sending successful shutdown signal from service {}",
                    S::SERVICE_ID
                );
            }
            debug!("{} {}", S::SERVICE_ID, "Shutting down service");
            true
        }
        LifecycleMessage::Kill => {
            debug!("{} {}", S::SERVICE_ID, "Killing service");
            true
        }
    }
}
