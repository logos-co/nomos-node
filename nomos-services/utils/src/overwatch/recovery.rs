// STD
use std::fmt::Debug;
use std::io;
// Crates
use crate::traits::FromSettings;
use log::error;
use overwatch_rs::services::state::{ServiceState, StateOperator};
use serde::{de::DeserializeOwned, Serialize};

type RecoveryResult<T> = Result<T, RecoveryError>;

#[derive(thiserror::Error, Debug)]
pub enum RecoveryError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}

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

#[cfg(test)]
mod tests {
    use super::*;
    use async_trait::async_trait;
    use overwatch_derive::Services;
    use overwatch_rs::overwatch::OverwatchRunner;
    use overwatch_rs::services::handle::{ServiceHandle, ServiceStateHandle};
    use overwatch_rs::services::relay::RelayMessage;
    use overwatch_rs::services::{ServiceCore, ServiceData, ServiceId};
    use overwatch_rs::DynError;
    use serde::Deserialize;
    use std::env::temp_dir;
    use std::marker::PhantomData;
    use std::path::PathBuf;

    #[derive(Debug, Clone)]
    struct FileBackend<State> {
        recovery_file: PathBuf,
        state: PhantomData<State>,
    }

    impl<State> FromSettings for FileBackend<State> {
        type Settings = SettingsWithRecovery;
        fn from_settings(settings: &Self::Settings) -> Self {
            Self {
                recovery_file: settings.recovery_file.clone(),
                state: Default::default(),
            }
        }
    }

    impl<State> RecoveryBackend for FileBackend<State>
    where
        State: ServiceState + Serialize + DeserializeOwned,
    {
        type State = State;

        fn load_state(&self) -> RecoveryResult<Self::State> {
            let serialized_state =
                std::fs::read_to_string(&self.recovery_file).map_err(RecoveryError::from)?;
            serde_json::from_str(&serialized_state).map_err(RecoveryError::from)
        }

        fn save_state(&self, state: &Self::State) -> RecoveryResult<()> {
            let deserialized_state = serde_json::to_string(state)?;
            std::fs::write(&self.recovery_file, deserialized_state).map_err(RecoveryError::from)
        }
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct MyState {
        value: String,
    }

    impl ServiceState for MyState {
        type Settings = SettingsWithRecovery;
        type Error = DynError;
        fn from_settings(_settings: &Self::Settings) -> Result<Self, DynError> {
            Ok(Self::default())
        }
    }

    #[derive(Debug)]
    pub enum MyMessage {}

    impl RelayMessage for MyMessage {}

    #[derive(Debug, Clone)]
    pub struct SettingsWithRecovery {
        recovery_file: PathBuf,
    }

    struct Recovery {
        service_state_handle: ServiceStateHandle<Self>,
    }

    impl ServiceData for Recovery {
        const SERVICE_ID: ServiceId = "RECOVERY_SERVICE";
        type Settings = SettingsWithRecovery;
        type State = MyState;
        type StateOperator = RecoveryOperator<FileBackend<Self::State>>;
        type Message = MyMessage;
    }

    #[async_trait]
    impl ServiceCore for Recovery {
        fn init(
            service_state: ServiceStateHandle<Self>,
            initial_state: Self::State,
        ) -> Result<Self, DynError> {
            assert_eq!(initial_state.value, "");
            Ok(Self {
                service_state_handle: service_state,
            })
        }

        async fn run(self) -> Result<(), DynError> {
            let Self {
                service_state_handle,
            } = self;

            service_state_handle.state_updater.update(Self::State {
                value: "Hello".to_string(),
            });

            service_state_handle.overwatch_handle.shutdown().await;
            Ok(())
        }
    }

    #[derive(Services)]
    pub struct RecoveryTest {
        recovery: ServiceHandle<Recovery>,
    }

    #[test]
    fn test_recovery() {
        // Initialize recovery file backend
        let recovery_file = temp_dir().join("recovery_test.json");
        let recovery_settings = SettingsWithRecovery { recovery_file };
        let file_backend: FileBackend<MyState> = FileBackend::from_settings(&recovery_settings);

        // Run the service with recovery enabled
        let service_settings = RecoveryTestServiceSettings {
            recovery: recovery_settings,
        };
        let app = OverwatchRunner::<RecoveryTest>::run(service_settings, None).unwrap();
        app.wait_finished();

        // Read contents of the recovery file
        let serialized_state = std::fs::read_to_string(&file_backend.recovery_file);

        // Early clean up (to avoid left over due to test failure)
        std::fs::remove_file(&file_backend.recovery_file).unwrap();

        // Verify the recovery file was created and contains the correct state
        assert!(serialized_state.is_ok());
        assert_eq!(serialized_state.unwrap(), "{\"value\":\"Hello\"}");
    }
}
