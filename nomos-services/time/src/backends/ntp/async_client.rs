use futures::stream::FuturesUnordered;
use futures::{FutureExt, StreamExt, TryStreamExt};
use sntpc::{get_time, Error as SntpError, NtpContext, NtpResult, StdTimestampGen};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::ToSocketAddrs;
use tokio::{
    net::{lookup_host, UdpSocket},
    time::{interval, timeout},
};

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(std::io::Error),
    #[error("NTP internal error: {0:?}")]
    Sntp(SntpError),
}

pub struct NTPClientSettings {
    timeout: Duration,
    address: SocketAddr,
    concurrency_limit: usize,
}
pub struct AsyncNTPClient {
    settings: NTPClientSettings,
    socket: UdpSocket,
    ntp_context: NtpContext<StdTimestampGen>,
}

impl AsyncNTPClient {
    pub async fn new(settings: NTPClientSettings) -> Self {
        let socket = UdpSocket::bind(settings.address)
            .await
            .expect("Socket binding for ntp client");
        let ntp_context = NtpContext::new(StdTimestampGen::default());
        Self {
            settings,
            socket,
            ntp_context,
        }
    }

    pub async fn request_timestamp<T: ToSocketAddrs>(&self, pool: T) -> Result<NtpResult, Error> {
        let hosts = lookup_host(&pool).await.map_err(Error::Io)?;
        let mut checks = FuturesUnordered::from_iter(
            hosts.map(move |host| get_time(host, &self.socket, self.ntp_context)),
        )
        .into_stream();
        checks.select_next_some().await.map_err(Error::Sntp)
    }
}
