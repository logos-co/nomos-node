use std::{fs::File, str::FromStr};

#[derive(Copy, Clone)]
pub enum LogFormat {
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

pub fn config_tracing<P: AsRef<std::path::Path>>(fmt: Option<LogFormat>, file: Option<&P>) {
    let filter = std::env::var("SIMULATION_LOG").unwrap_or_else(|_| "info".to_owned());
    if let Some(LogFormat::Json) = fmt {
        let subscriber = tracing_subscriber::fmt::fmt()
            .without_time()
            .with_line_number(true)
            .with_env_filter(filter)
            .with_file(false)
            .with_target(true)
            .json();

        if let Some(p) = file {
            let file = File::create(p).expect("Unable to create log file");
            tracing::subscriber::set_global_default(
                subscriber.with_ansi(false).with_writer(file).finish(),
            )
            .expect("config_tracing is only called once");
            return;
        }

        tracing::subscriber::set_global_default(subscriber.finish())
            .expect("config_tracing is only called once");
    } else {
        let subscriber = tracing_subscriber::fmt::fmt()
            .without_time()
            .with_line_number(true)
            .with_env_filter(filter)
            .with_file(false)
            .with_target(true);

        if let Some(p) = file {
            let file = File::create(p).expect("Unable to create log file");
            tracing::subscriber::set_global_default(
                subscriber.with_ansi(false).with_writer(file).finish(),
            )
            .expect("config_tracing is only called once");
            return;
        }

        tracing::subscriber::set_global_default(subscriber.finish())
            .expect("config_tracing is only called once");
    }
}
