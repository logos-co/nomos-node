use overwatch::services::{life_cycle::LifecycleMessage, ServiceData};
use tracing::{debug, error};

/// Handles the shutdown signal from `Overwatch`
pub fn should_stop_service<S: ServiceData>(msg: &LifecycleMessage) -> bool {
    match msg {
        LifecycleMessage::Start => {
            debug!("{} {}", S::SERVICE_ID, "Starting up service");
            false
        }
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
