// Crates
use serde::de::DeserializeOwned;
use serde::Serialize;
// Internal
use crate::overwatch::recovery::RecoveryResult;
use crate::overwatch::RecoveryError;

pub trait RecoverySerializer<State: Serialize + DeserializeOwned> {
    fn serialize(state: &State) -> RecoveryResult<String>;
    fn deserialize(serialized_state: &str) -> RecoveryResult<State>;
}

#[derive(Debug, Clone)]
pub struct JsonRecoverySerializer;

impl<State: Serialize + DeserializeOwned> RecoverySerializer<State> for JsonRecoverySerializer {
    fn serialize(state: &State) -> RecoveryResult<String> {
        serde_json::to_string(state).map_err(RecoveryError::from)
    }

    fn deserialize(serialized_state: &str) -> RecoveryResult<State> {
        serde_json::from_str(serialized_state).map_err(RecoveryError::from)
    }
}
