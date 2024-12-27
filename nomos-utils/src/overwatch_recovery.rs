// STD
use std::fmt::Debug;
use std::io;
use std::marker::PhantomData;
// Crates
use log::error;
use overwatch_rs::services::state::{ServiceState, StateOperator};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};

trait FromSettings {
    type Settings;
    fn from_settings(settings: &Self::Settings) -> Self;
}

#[derive(thiserror::Error, Debug)]
pub enum MyError {
    #[error(transparent)]
    IoError(#[from] io::Error),
    #[error(transparent)]
    SerdeError(#[from] serde_json::Error),
}

type _Result<T> = Result<T, MyError>;

#[derive(Serialize, Deserialize, Debug, Clone)]
struct RecoverableState<Backend, Settings>
where
    Backend: RecoveryBackend + Debug + Clone,
    Backend::State: Serialize + DeserializeOwned + Debug + Clone,
{
    #[serde(flatten)]
    state: <Backend as RecoveryBackend>::State,
    #[serde(skip)]
    settings: PhantomData<Settings>,
    #[serde(skip)]
    backend: PhantomData<Backend>,
}

impl<Backend, Settings> RecoverableState<Backend, Settings>
where
    Backend: RecoveryBackend + Debug + Clone,
    Backend::State: Serialize + DeserializeOwned + Debug + Clone,
{
    fn new(state: <Backend as RecoveryBackend>::State) -> Self {
        Self {
            state,
            settings: Default::default(),
            backend: Default::default(),
        }
    }
}

impl<Backend, Settings> ServiceState for RecoverableState<Backend, Settings>
where
    Backend: RecoveryBackend + FromSettings<Settings = Settings> + Debug + Clone,
    Backend::State: Default + Serialize + DeserializeOwned + Debug + Clone,
{
    type Settings = Settings;
    type Error = MyError;

    fn from_settings(settings: &Self::Settings) -> Result<Self, Self::Error> {
        let state = Backend::from_settings(settings)
            .load_state()
            .unwrap_or_default();
        Ok(Self {
            state,
            settings: Default::default(),
            backend: Default::default(),
        })
    }
}

trait RecoveryBackend {
    type State;
    fn load_state(&self) -> _Result<Self::State>;
    fn save_state(&self, state: &Self::State) -> _Result<()>;
}

#[derive(Debug, Clone)]
struct RecoveryOperator<Backend, Settings>
where
    Backend: RecoveryBackend + Clone + Debug,
    Backend::State: Clone + Debug,
    Settings: Clone + Debug,
{
    recovery_backend: Backend,
    settings: PhantomData<Settings>,
}

impl<Backend: RecoveryBackend, Settings> RecoveryOperator<Backend, Settings>
where
    Backend: RecoveryBackend + FromSettings + Clone + Debug,
    Backend::State: Clone + Debug,
    Settings: Clone + Debug,
{
    fn new(recovery_backend: Backend) -> Self {
        Self {
            recovery_backend,
            settings: Default::default(),
        }
    }
}

#[async_trait::async_trait]
impl<Backend, Settings> StateOperator for RecoveryOperator<Backend, Settings>
where
    Settings: Send + Debug + Clone,
    Backend: RecoveryBackend + FromSettings<Settings = Settings> + Debug + Clone + Send,
    Backend::State: Serialize
        + DeserializeOwned
        + FromSettings<Settings = Settings>
        + Send
        + Default
        + Debug
        + Clone,
{
    type StateInput = RecoverableState<Backend, Settings>;

    fn from_settings(settings: <Self::StateInput as ServiceState>::Settings) -> Self {
        let recovery_backend = Backend::from_settings(&settings);
        Self::new(recovery_backend)
    }

    async fn run(&mut self, state: Self::StateInput) {
        let save_result = self
            .recovery_backend
            .save_state(&state.state)
            .map_err(MyError::from);
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
    use std::env::temp_dir;
    use std::path::PathBuf;

    #[derive(Debug, Clone)]
    struct FileBackend<State> {
        directory: PathBuf,
        state: PhantomData<State>,
    }

    impl<State> FromSettings for FileBackend<State> {
        type Settings = MySettings;
        fn from_settings(settings: &Self::Settings) -> Self {
            Self {
                directory: settings.recovery_directory.clone(),
                state: Default::default(),
            }
        }
    }

    impl<State> FileBackend<State> {
        fn recovery_file(&self) -> PathBuf {
            self.directory.join("state.json")
        }
    }

    impl<State> RecoveryBackend for FileBackend<State>
    where
        State: Serialize + DeserializeOwned,
    {
        type State = State;

        fn load_state(&self) -> _Result<Self::State> {
            let serialized_state =
                std::fs::read_to_string(self.recovery_file()).map_err(MyError::from)?;
            serde_json::from_str(&serialized_state).map_err(MyError::from)
        }

        fn save_state(&self, state: &Self::State) -> _Result<()> {
            let deserialized_state = serde_json::to_string(state)?;
            std::fs::write(self.recovery_file(), deserialized_state).map_err(MyError::from)
        }
    }

    #[derive(Debug, Clone, Default, Serialize, Deserialize)]
    struct MyActualState {
        value: String,
    }

    #[derive(Debug)]
    pub enum MyMessage {}

    impl RelayMessage for MyMessage {}

    #[derive(Debug, Clone)]
    pub struct MySettings {
        recovery_directory: PathBuf,
    }

    impl FromSettings for MyActualState {
        type Settings = MySettings;
        fn from_settings(_settings: &Self::Settings) -> Self {
            Self {
                value: "Hello".to_string(),
            }
        }
    }

    struct Recovery {
        service_state_handle: ServiceStateHandle<Self>,
    }

    impl ServiceData for Recovery {
        const SERVICE_ID: ServiceId = "RECOVERY_SERVICE";
        type Settings = MySettings;
        type State = RecoverableState<FileBackend<MyActualState>, Self::Settings>;
        type StateOperator = RecoveryOperator<FileBackend<MyActualState>, Self::Settings>;
        type Message = MyMessage;
    }

    #[async_trait]
    impl ServiceCore for Recovery {
        fn init(
            service_state: ServiceStateHandle<Self>,
            initial_state: Self::State,
        ) -> Result<Self, DynError> {
            println!("Initial State: {:#?}", initial_state);
            Ok(Self {
                service_state_handle: service_state,
            })
        }

        async fn run(self) -> Result<(), DynError> {
            let Self {
                service_state_handle,
            } = self;

            service_state_handle
                .state_updater
                .update(Self::State::new(MyActualState {
                    value: "Hello".to_string(),
                }));

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
        // Initialize the recovery settings
        let recovery_directory = temp_dir();
        let recovery_settings = MySettings {
            recovery_directory: recovery_directory.clone(),
        };

        // Verify that the recovery file does not exist
        let file_backend: FileBackend<MyActualState> =
            FileBackend::from_settings(&recovery_settings);
        assert!(!file_backend.recovery_file().exists());

        // Run the recovery service
        let service_settings = RecoveryTestServiceSettings {
            recovery: recovery_settings,
        };
        let app = OverwatchRunner::<RecoveryTest>::run(service_settings, None).unwrap();
        app.wait_finished();

        // Verify the recovery file content
        assert!(file_backend.recovery_file().exists());
        let serialized_state = std::fs::read_to_string(file_backend.recovery_file()).unwrap();
        assert_eq!(serialized_state, "{\"value\":\"Hello\"}");

        // Clean up
        std::fs::remove_file(file_backend.recovery_file()).unwrap();
    }
}
