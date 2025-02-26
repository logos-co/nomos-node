use futures::stream::FuturesUnordered;
use futures::{StreamExt, TryStreamExt};
use sntpc::{get_time, Error as SntpError, NtpContext, NtpResult, StdTimestampGen};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::ToSocketAddrs;
use tokio::net::{lookup_host, UdpSocket};
use tokio::time::error::Elapsed;
use tokio::time::timeout;

#[derive(thiserror::Error, Debug)]
pub enum Error {
    #[error(transparent)]
    Io(std::io::Error),
    #[error("NTP internal error: {0:?}")]
    Sntp(SntpError),
    #[error("NTP request timeout, elapsed: {0:?}")]
    Timeout(Elapsed),
}

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Copy, Clone)]
pub struct NTPClientSettings {
    /// NTP server requests timeout duration
    timeout: Duration,
    /// NTP server socket address
    address: SocketAddr,
}
#[derive(Clone)]
pub struct AsyncNTPClient {
    settings: NTPClientSettings,
    ntp_context: NtpContext<StdTimestampGen>,
}

impl AsyncNTPClient {
    pub fn new(settings: NTPClientSettings) -> Self {
        let ntp_context = NtpContext::new(StdTimestampGen::default());
        Self {
            settings,
            ntp_context,
        }
    }

    /// Request a timestamp from an NTP server
    pub async fn request_timestamp<T: ToSocketAddrs>(&self, pool: T) -> Result<NtpResult, Error> {
        let socket = &UdpSocket::bind(self.settings.address)
            .await
            .map_err(Error::Io)?;
        let hosts = lookup_host(&pool).await.map_err(Error::Io)?;
        let mut checks = FuturesUnordered::from_iter(
            hosts.map(move |host| get_time(host, socket, self.ntp_context)),
        )
        .into_stream();
        timeout(self.settings.timeout, checks.select_next_some())
            .await
            .map_err(Error::Timeout)?
            .map_err(Error::Sntp)
    }
}
