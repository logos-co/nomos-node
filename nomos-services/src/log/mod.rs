use overwatch::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData,
};
use std::net::SocketAddr;
use std::path::PathBuf;
use tracing::Level;
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::LevelFilter, prelude::*};

pub struct Logger(Option<WorkerGuard>);

#[derive(Clone, Debug)]
pub enum LoggerBackend {
    Gelf {
        addr: SocketAddr,
    },
    File {
        directory: PathBuf,
        prefix: Option<PathBuf>,
    },
    Stdout,
    Stderr,
}

#[derive(Clone, Debug)]
pub struct LoggerSettings {
    backend: LoggerBackend,
    format: LoggerFormat,
    level: Level,
}

#[derive(Clone, Debug)]
pub enum LoggerFormat {
    Json,
    Plain,
}

impl ServiceData for Logger {
    const SERVICE_ID: &'static str = "Logger";
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
    type Settings = LoggerSettings;
}

// a macro and not a function because it's a bit of a type
// mess with `Layer<S>`
macro_rules! registry_init {
    ($layer:expr, $format:expr, $level:expr) => {
        if let LoggerFormat::Json = $format {
            tracing_subscriber::registry()
                .with(LevelFilter::from($level))
                .with($layer)
                .init();
        } else {
            tracing_subscriber::registry()
                .with(LevelFilter::from($level))
                .with($layer)
                .init();
        }
    };
}

#[async_trait::async_trait]
impl ServiceCore for Logger {
    fn init(mut service_state: ServiceStateHandle<Self>) -> Self {
        let config = service_state.settings_reader.get_updated_settings();
        let (non_blocking, _guard) = match config.backend {
            LoggerBackend::Gelf { addr } => {
                let (layer, mut task) = tracing_gelf::Logger::builder().connect_tcp(addr).unwrap();
                service_state
                    .overwatch_handle
                    .runtime()
                    .spawn(async move { task.connect().await });
                registry_init!(layer, config.format, config.level);
                return Self(None);
            }
            LoggerBackend::File { directory, prefix } => {
                let file_appender = tracing_appender::rolling::hourly(
                    directory,
                    prefix.unwrap_or_else(|| PathBuf::from("nomos.log")),
                );
                tracing_appender::non_blocking(file_appender)
            }
            LoggerBackend::Stdout => tracing_appender::non_blocking(std::io::stdout()),
            LoggerBackend::Stderr => tracing_appender::non_blocking(std::io::stderr()),
        };

        let layer = tracing_subscriber::fmt::Layer::new()
            .with_level(true)
            .with_writer(non_blocking);
        registry_init!(layer, config.format, config.level);
        Self(Some(_guard))
    }

    async fn run(self) {
        // keep the handle alive without stressing the runtime
        futures::pending!()
    }
}
