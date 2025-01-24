// STD
use std::fmt::Debug;
// Crates
use log::error;
use overwatch_rs::services::state::{ServiceState, StateOperator};
use serde::{de::DeserializeOwned, Serialize};
// Internal
use crate::overwatch::recovery::errors::RecoveryError;
use crate::overwatch::recovery::RecoveryResult;
use crate::traits::FromSettings;

pub trait RecoveryBackend {
    type State: ServiceState;
    fn load_state(&self) -> RecoveryResult<Self::State>;
    fn save_state(&self, state: &Self::State) -> RecoveryResult<()>;
}

#[derive(Debug, Clone)]
pub struct RecoveryOperator<Backend>
where
    Backend: RecoveryBackend + Debug + Clone,
    Backend::State: Debug + Clone,
{
    recovery_backend: Backend,
}

impl<Backend: RecoveryBackend> RecoveryOperator<Backend>
where
    Backend: RecoveryBackend + FromSettings + Debug + Clone,
    Backend::State: Debug + Clone,
{
    fn new(recovery_backend: Backend) -> Self {
        Self { recovery_backend }
    }
}

#[async_trait::async_trait]
impl<Backend> StateOperator for RecoveryOperator<Backend>
where
    Backend: RecoveryBackend
        + FromSettings<Settings = <Backend::State as ServiceState>::Settings>
        + Send
        + Debug
        + Clone,
    Backend::State: Serialize + DeserializeOwned + Default + Send + Debug + Clone,
{
    type StateInput = Backend::State;
    type LoadError = RecoveryError;

    fn try_load(
        settings: &<Self::StateInput as ServiceState>::Settings,
    ) -> Result<Option<Self::StateInput>, Self::LoadError> {
        Backend::from_settings(settings)
            .load_state()
            .map(Option::from)
            .map_err(RecoveryError::from)
    }

    fn from_settings(settings: <Self::StateInput as ServiceState>::Settings) -> Self {
        let recovery_backend = Backend::from_settings(&settings);
        Self::new(recovery_backend)
    }

    async fn run(&mut self, state: Self::StateInput) {
        let save_result = self
            .recovery_backend
            .save_state(&state)
            .map_err(RecoveryError::from);
        if let Err(error) = save_result {
            error!("{}", error);
        }
    }
}
