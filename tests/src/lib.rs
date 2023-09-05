mod nodes;
pub use nodes::NomosNode;
use once_cell::sync::Lazy;

// std
use std::net::TcpListener;
use std::time::Duration;
use std::{fmt::Debug, sync::Mutex};

//crates
use fraction::Fraction;
use rand::{thread_rng, Rng};

pub fn initialize_tracing() {
    use std::sync::Once;
    static TRACE: Once = Once::new();
    TRACE.call_once(|| {
        let filter = std::env::var("SHOWBIZ_TESTING_LOG").unwrap_or_else(|_| "debug".to_owned());
        tracing::subscriber::set_global_default(
            tracing_subscriber::fmt::fmt()
                .without_time()
                .with_line_number(true)
                .with_env_filter(filter)
                .with_file(false)
                .with_target(true)
                .with_ansi(true)
                .finish(),
        )
        .unwrap();
    });
}

pub fn run_test(fut: impl std::future::Future<Output = ()>) {
    initialize_tracing();
    ::tokio::runtime::Runtime::new().unwrap().block_on(fut);
}

pub fn run_test_multiple_threads(num_threads: usize, fut: impl std::future::Future<Output = ()>) {
    initialize_tracing();
    tokio::runtime::Builder::new_multi_thread()
        .worker_threads(num_threads)
        .enable_all()
        .build()
        .unwrap()
        .block_on(fut);
}

static NET_PORT: Lazy<Mutex<u16>> = Lazy::new(|| Mutex::new(thread_rng().gen_range(8000..10000)));

pub fn get_available_port() -> u16 {
    let mut port = NET_PORT.lock().unwrap();
    *port += 1;
    while TcpListener::bind(("127.0.0.1", *port)).is_err() {
        *port += 1;
    }
    *port
}

#[async_trait::async_trait]
pub trait Node: Sized {
    type ConsensusInfo: Debug + Clone + PartialEq;
    async fn spawn_nodes(config: SpawnConfig) -> Vec<Self>;
    async fn consensus_info(&self) -> Self::ConsensusInfo;
    fn stop(&mut self);
}

#[derive(Clone, Copy)]
pub enum SpawnConfig {
    Star {
        n_participants: usize,
        threshold: Fraction,
        timeout: Duration,
    },
}
