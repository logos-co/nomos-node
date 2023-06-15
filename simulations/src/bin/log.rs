use std::str::FromStr;

#[derive(Copy, Clone)]
pub(super) enum LogFormat {
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

pub(super) fn config_tracing(fmt: Option<LogFormat>) {
    let filter = std::env::var("SIMULATION_LOG").unwrap_or_else(|_| "info".to_owned());
    if let Some(LogFormat::Json) = fmt {
        let subscriber = tracing_subscriber::fmt::fmt()
            .without_time()
            .with_line_number(true)
            .with_env_filter(filter)
            .with_file(false)
            .with_target(true)
            .with_ansi(true)
            .json()
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("config_tracing is only called once");
    } else {
        let subscriber = tracing_subscriber::fmt::fmt()
            .without_time()
            .with_line_number(true)
            .with_env_filter(filter)
            .with_file(false)
            .with_target(true)
            .with_ansi(true)
            .finish();
        tracing::subscriber::set_global_default(subscriber)
            .expect("config_tracing is only called once");
    }
}
