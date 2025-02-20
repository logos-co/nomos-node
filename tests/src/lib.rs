pub mod nodes;
pub mod topology;

// std
use std::env;
use std::net::TcpListener;
use std::ops::Mul;
use std::sync::Mutex;
use std::time::Duration;

//crates
use nomos_libp2p::{Multiaddr, PeerId, Swarm};
use once_cell::sync::Lazy;
use rand::{thread_rng, Rng};

static NET_PORT: Lazy<Mutex<u16>> = Lazy::new(|| Mutex::new(thread_rng().gen_range(8000..10000)));

static IS_SLOW_TEST_ENV: Lazy<bool> =
    Lazy::new(|| env::var("SLOW_TEST_ENV").is_ok_and(|s| s == "true"));

pub static GLOBAL_PARAMS_PATH: Lazy<String> = Lazy::new(|| {
    let relative_path = "./kzgrs/kzgrs_test_params";
    let current_dir = env::current_dir().expect("Failed to get current directory");
    current_dir
        .join(relative_path)
        .canonicalize()
        .expect("Failed to resolve absolute path")
        .to_string_lossy()
        .to_string()
});

/// Global flag indicating whether debug tracing configuration is enabled to send traces to local
/// grafana stack.
pub static IS_DEBUG_TRACING: Lazy<bool> =
    Lazy::new(|| env::var("NOMOS_TESTS_TRACING").is_ok_and(|val| val.eq_ignore_ascii_case("true")));

pub fn get_available_port() -> u16 {
    let mut port = NET_PORT.lock().unwrap();
    *port += 1;
    while TcpListener::bind(("127.0.0.1", *port)).is_err() {
        *port += 1;
    }
    *port
}

/// In slow test environments like Codecov, use 2x timeout.
pub fn adjust_timeout(d: Duration) -> Duration {
    if *IS_SLOW_TEST_ENV {
        d.mul(2)
    } else {
        d
    }
}

fn node_address_from_port(port: u16) -> Multiaddr {
    Swarm::multiaddr(std::net::Ipv4Addr::new(127, 0, 0, 1), port)
}

fn secret_key_to_peer_id(node_key: nomos_libp2p::ed25519::SecretKey) -> PeerId {
    PeerId::from_public_key(
        &nomos_libp2p::ed25519::Keypair::from(node_key)
            .public()
            .into(),
    )
}
