// std
use futures::StreamExt;
use std::fmt::{Debug, Formatter};
use std::io::Write;
use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;
// crates
use serde::{Deserialize, Serialize};
use tracing::{error, Level};
use tracing_appender::non_blocking::WorkerGuard;
use tracing_subscriber::{filter::LevelFilter, prelude::*};
// internal
use overwatch_rs::services::life_cycle::LifecycleMessage;
use overwatch_rs::services::{
    handle::ServiceStateHandle,
    relay::NoMessage,
    state::{NoOperator, NoState},
    ServiceCore, ServiceData,
};

const GELF_RECONNECT_INTERVAL: u64 = 10;

pub struct Logger {
    service_state: ServiceStateHandle<Self>,
    worker_guard: Option<WorkerGuard>,
}

/// This is a wrapper around a writer to allow cloning which is
/// required by contract by Overwatch for a configuration struct
#[derive(Clone)]
pub struct SharedWriter {
    inner: Arc<Mutex<dyn Write + Send + Sync>>,
}

impl Write for SharedWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        self.inner.lock().unwrap().write(buf)
    }

    fn flush(&mut self) -> std::io::Result<()> {
        self.inner.lock().unwrap().flush()
    }
}

impl SharedWriter {
    pub fn new<W: Write + Send + Sync + 'static>(writer: W) -> Self {
        Self {
            inner: Arc::new(Mutex::new(writer)),
        }
    }

    pub fn into_inner(&self) -> Arc<Mutex<dyn Write + Send + Sync>> {
        self.inner.clone()
    }

    pub fn from_inner(inner: Arc<Mutex<dyn Write + Send + Sync>>) -> Self {
        Self { inner }
    }
}

impl Debug for SharedWriter {
    fn fmt(&self, f: &mut Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SharedWriter").finish()
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
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
    #[serde(skip)]
    Writer(SharedWriter),
    // do not collect logs
    None,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LoggerSettings {
    pub backend: LoggerBackend,
    pub format: LoggerFormat,
    #[serde(with = "serde_level")]
    pub level: Level,
}

impl Default for LoggerSettings {
    fn default() -> Self {
        Self {
            backend: LoggerBackend::Stdout,
            format: LoggerFormat::Json,
            level: Level::DEBUG,
        }
    }
}

impl LoggerSettings {
    #[inline]
    pub const fn new(backend: LoggerBackend, format: LoggerFormat, level: Level) -> Self {
        Self {
            backend,
            format,
            level,
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum LoggerFormat {
    Json,
    Plain,
}

impl ServiceData for Logger {
    const SERVICE_ID: &'static str = "Logger";
    type Settings = LoggerSettings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
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
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, overwatch_rs::DynError> {
        #[cfg(test)]
        use std::sync::Once;
        #[cfg(test)]
        static ONCE_INIT: Once = Once::new();

        let config = service_state.settings_reader.get_updated_settings();
        let (non_blocking, _guard) = match config.backend {
            LoggerBackend::Gelf { addr } => {
                let (layer, mut task) = tracing_gelf::Logger::builder()
                    .connect_tcp(addr)
                    .expect("Connect to the graylog instance");
                service_state.overwatch_handle.runtime().spawn(async move {
                    loop {
                        if task.connect().await.0.is_empty() {
                            break;
                        } else {
                            eprintln!("Failed to connect to graylog");
                            let delay = Duration::from_secs(GELF_RECONNECT_INTERVAL);
                            tokio::time::sleep(delay).await;
                        }
                    }
                });
                #[cfg(test)]
                ONCE_INIT.call_once(move || {
                    registry_init!(layer, config.format, config.level);
                });
                #[cfg(not(test))]
                registry_init!(layer, config.format, config.level);

                return Ok(Self {
                    service_state,
                    worker_guard: None,
                });
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
            LoggerBackend::Writer(writer) => tracing_appender::non_blocking(writer),
            LoggerBackend::None => {
                return Ok(Self {
                    service_state,
                    worker_guard: None,
                })
            }
        };

        let layer = tracing_subscriber::fmt::Layer::new()
            .with_level(true)
            .with_writer(non_blocking);
        #[cfg(test)]
        ONCE_INIT.call_once(move || {
            registry_init!(layer, config.format, config.level);
        });
        #[cfg(not(test))]
        registry_init!(layer, config.format, config.level);

        Ok(Self {
            service_state,
            worker_guard: Some(_guard),
        })
    }

    async fn run(self) -> Result<(), overwatch_rs::DynError> {
        let Self {
            service_state,
            worker_guard,
        } = self;
        // keep the handle alive without stressing the runtime
        let mut lifecycle_stream = service_state.lifecycle_handle.message_stream();
        loop {
            if let Some(msg) = lifecycle_stream.next().await {
                match msg {
                    LifecycleMessage::Shutdown(sender) => {
                        // flush pending logs before signaling message processing
                        drop(worker_guard);
                        if sender.send(()).is_err() {
                            error!(
                                "Error sending successful shutdown signal from service {}",
                                Self::SERVICE_ID
                            );
                        }
                        break;
                    }
                    LifecycleMessage::Kill => {
                        break;
                    }
                }
            }
        }
        Ok(())
    }
}

mod serde_level {
    use super::Level;
    use serde::{de::Error, Deserialize, Deserializer, Serialize, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Level, D::Error>
    where
        D: Deserializer<'de>,
    {
        <String>::deserialize(deserializer).and_then(|v| {
            v.parse()
                .map_err(|e| D::Error::custom(format!("invalid log level {e}")))
        })
    }

    pub fn serialize<S>(value: &Level, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        value.as_str().serialize(serializer)
    }
}
