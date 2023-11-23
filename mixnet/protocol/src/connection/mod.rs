mod conn;
mod pool;

pub use conn::{ConnectionCommand, ConnectionConfig};
pub use pool::{Command, ConnectionPool, ConnectionPoolConfig};

#[cfg(test)]
mod tests {
    use once_cell::sync::Lazy;
    use rand::{thread_rng, Rng};
    use sphinx_packet::payload::{Payload, PAYLOAD_OVERHEAD_SIZE};
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};
    use std::sync::Mutex;
    use tokio::io::AsyncReadExt;
    use tokio::{net::TcpListener, sync::oneshot};

    use crate::connection::conn::ConnectionCommand;
    use crate::connection::pool::{Command, ConnectionPool, ConnectionPoolConfig};
    use crate::Body;

    static NET_PORT: Lazy<Mutex<u16>> =
        Lazy::new(|| Mutex::new(thread_rng().gen_range(8000..10000)));

    fn get_available_port() -> u16 {
        let mut port = NET_PORT.lock().unwrap();
        *port += 1;
        while std::net::TcpListener::bind(("127.0.0.1", *port)).is_err() {
            *port += 1;
        }
        *port
    }

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn write() {
        // prepare parameters
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            get_available_port(),
        );
        let data = [9u8; PAYLOAD_OVERHEAD_SIZE];

        // open a server
        let listener = TcpListener::bind(addr).await.unwrap();

        // send a write command
        let mut pool = ConnectionPool::new(ConnectionPoolConfig::default());
        let (tx, rx) = oneshot::channel();
        pool.submit(Command {
            addr,
            command: ConnectionCommand::Write {
                body: Body::new_final_payload(Payload::from_bytes(&data).unwrap()),
                tx,
            },
        });
        rx.await.unwrap().unwrap();

        // check if body is arrived in the server
        let (mut svr_stream, _) = listener.accept().await.unwrap();
        // since the actual data arrived depends on how the payload write is implemented,
        // here we just check if something has been arrived.
        let mut buf = [0u8; 1];
        assert_eq!(svr_stream.read_exact(&mut buf).await.unwrap(), 1);
    }

    #[tokio::test]
    #[tracing_test::traced_test]
    async fn reconnect() {
        // prepare parameters
        let addr = SocketAddr::new(
            IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)),
            get_available_port(),
        );
        let data = [9u8; PAYLOAD_OVERHEAD_SIZE];

        // expect to be failed because there is no server listening
        let mut pool = ConnectionPool::new(ConnectionPoolConfig::default());
        let (tx, rx) = oneshot::channel();
        pool.submit(Command {
            addr,
            command: ConnectionCommand::Write {
                body: Body::new_final_payload(Payload::from_bytes(&data).unwrap()),
                tx,
            },
        });
        assert!(rx.await.unwrap().is_err());

        // open a server
        let _listener = TcpListener::bind(addr).await.unwrap();

        // expect to be succeeded with a server listening
        let (tx, rx) = oneshot::channel();
        pool.submit(Command {
            addr,
            command: ConnectionCommand::Write {
                body: Body::new_final_payload(Payload::from_bytes(&data).unwrap()),
                tx,
            },
        });
        rx.await.unwrap().unwrap();
    }
}
