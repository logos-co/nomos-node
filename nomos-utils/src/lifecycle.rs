use overwatch_rs::services::life_cycle::LifecycleMessage;
use tracing::{debug, error};

/// Handles the shutdown signal from `Overwatch`
pub async fn should_stop_service(msg: &LifecycleMessage, service_id: &str) -> bool {
    match msg {
        LifecycleMessage::Shutdown(sender) => {
            if sender.send(()).is_err() {
                error!("Error sending successful shutdown signal from service {service_id}",);
            }
            debug!(service_id, "Shutting down service");
            true
        }
        LifecycleMessage::Kill => {
            debug!(service_id, "Killing service");
            true
        }
    }
}
