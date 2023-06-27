use std::{
    fs::File,
    io::{stderr, stdout},
    path::PathBuf,
    str::FromStr,
};
use tracing_subscriber::fmt::{format::Format, FormatEvent, FormatFields, SubscriberBuilder};

#[derive(Default, Copy, Clone)]
pub enum LogFormat {
    #[default]
    Plain,
    Json,
}

impl FromStr for LogFormat {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "json" => Ok(LogFormat::Json),
            "plain" => Ok(LogFormat::Plain),
            _ => Err(anyhow::anyhow!("Unknown log format")),
        }
    }
}

#[derive(Default, Clone)]
pub enum LogOutput {
    #[default]
    StdOut,
    StdErr,
    File(PathBuf),
}

impl FromStr for LogOutput {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.trim().to_ascii_lowercase().as_str() {
            "stdout" => Ok(Self::StdOut),
            "stderr" => Ok(Self::StdErr),
            path => Ok(Self::File(PathBuf::from(path))),
        }
    }
}

pub fn config_tracing(fmt: LogFormat, file: &LogOutput) {
    let filter = std::env::var("SIMULATION_LOG").unwrap_or_else(|_| "info".to_owned());

    let subscriber = tracing_subscriber::fmt::fmt()
        .without_time()
        .with_line_number(true)
        .with_env_filter(filter)
        .with_file(false)
        .with_target(true);

    if let LogFormat::Json = fmt {
        set_global(subscriber.json(), file);
    } else {
        set_global(subscriber, file);
    }
}

fn set_global<N, L, T>(
    subscriber: SubscriberBuilder<N, Format<L, T>, tracing_subscriber::EnvFilter>,
    output: &LogOutput,
) where
    N: for<'writer> FormatFields<'writer> + 'static + Send + Sync,
    Format<L, T>: FormatEvent<tracing_subscriber::Registry, N>,
    L: Send + Sync + 'static,
    T: Send + Sync + 'static,
{
    use tracing::subscriber::set_global_default;
    match output {
        LogOutput::StdOut => set_global_default(subscriber.with_writer(stdout).finish()),
        LogOutput::StdErr => set_global_default(subscriber.with_writer(stderr).finish()),
        LogOutput::File(path) => set_global_default(
            subscriber
                .with_ansi(false)
                .with_writer(File::create(path).expect("Unable to create log file"))
                .finish(),
        ),
    }
    .expect("Unable to set global default subscriber")
}
