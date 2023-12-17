use crate::api::mempool::send_certificate;
use clap::{Args, ValueEnum};
use full_replication::{AbsoluteNumber, Attestation, Certificate, FullReplication, Voter};
use futures::StreamExt;
use hex::FromHex;
use nomos_core::{crypto::PrivateKey, da::DaProtocol, wire};
use nomos_da::network::{adapters::libp2p::Libp2pAdapter, NetworkAdapter};
use nomos_network::{backends::libp2p::Libp2p, NetworkService};
use overwatch_derive::*;
use overwatch_rs::{
    services::{
        handle::{ServiceHandle, ServiceStateHandle},
        relay::NoMessage,
        state::*,
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};
use reqwest::Url;
use serde::Serialize;
use std::{
    error::Error,
    path::PathBuf,
    sync::{mpsc::Sender, Arc},
    time::Duration,
};
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};

pub async fn disseminate_and_wait<D, B, N, A, C, Auth>(
    mut da: D,
    data: Box<[u8]>,
    adapter: N,
    status_updates: Sender<Status>,
    node_addr: Option<&Url>,
    output: Option<&PathBuf>,
    private_key: &PrivateKey,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    D: DaProtocol<Blob = B, Attestation = A, Certificate = C, Auth = Auth>,
    Auth: From<PrivateKey>,
    N: NetworkAdapter<Blob = B, Attestation = A> + Send + Sync,
    C: Serialize,
    B: nomos_core::da::blob::Blob,
    <B as nomos_core::da::blob::Blob>::Sender: From<[u8; 32]>,
{
    // 1) Building blob
    status_updates.send(Status::Encoding)?;
    let blobs = da.encode(Auth::from(*private_key), data);

    // 2) Send blob to network
    status_updates.send(Status::Disseminating)?;
    futures::future::try_join_all(blobs.into_iter().map(|blob| adapter.send_blob(blob)))
        .await
        .map_err(|e| e as Box<dyn std::error::Error + Sync + Send>)?;

    // 3) Collect attestations and create proof
    status_updates.send(Status::WaitingAttestations)?;
    let mut attestations = adapter.attestation_stream().await;
    let cert: C = loop {
        da.recv_attestation(attestations.next().await.unwrap());

        if let Some(certificate) = da.certify_dispersal() {
            status_updates.send(Status::CreatingCert)?;
            break certificate;
        }
    };

    if let Some(output) = output {
        status_updates.send(Status::SavingCert)?;
        std::fs::write(output, wire::serialize(&cert)?)?;
    }

    if let Some(node) = node_addr {
        status_updates.send(Status::SendingCert)?;
        let res = send_certificate(node, &cert).await?;

        if !res.status().is_success() {
            tracing::error!("ERROR: {:?}", res);
            return Err(format!("Failed to send certificate to node: {}", res.status()).into());
        }
    }

    status_updates.send(Status::Done)?;
    Ok(())
}

pub enum Status {
    Encoding,
    Disseminating,
    WaitingAttestations,
    CreatingCert,
    SavingCert,
    SendingCert,
    Done,
    Err(Box<dyn std::error::Error + Send + Sync>),
}

impl Status {
    pub fn display(&self) -> &str {
        match self {
            Self::Encoding => "Encoding message into blob(s)",
            Self::Disseminating => "Sending blob(s) to the network",
            Self::WaitingAttestations => "Waiting for attestations",
            Self::CreatingCert => "Creating certificate",
            Self::SavingCert => "Saving certificate to file",
            Self::SendingCert => "Sending certificate to node",
            Self::Done => "",
            Self::Err(_) => "Error",
        }
    }
}

// To interact with the network service it's easier to just spawn
// an overwatch app
#[derive(Services)]
pub struct DisseminateApp {
    network: ServiceHandle<NetworkService<Libp2p>>,
    send_blob: ServiceHandle<DisseminateService>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    // This is wrapped in an Arc just to make the struct Clone
    pub payload: Arc<Mutex<UnboundedReceiver<Box<[u8]>>>>,
    pub timeout: Duration,
    pub da_protocol: DaProtocolChoice,
    pub status_updates: Sender<Status>,
    pub node_addr: Option<Url>,
    pub output: Option<std::path::PathBuf>,
    pub private_key: PrivateKey,
}

pub struct DisseminateService {
    service_state: ServiceStateHandle<Self>,
}

impl ServiceData for DisseminateService {
    const SERVICE_ID: ServiceId = "Disseminate";
    type Settings = Settings;
    type State = NoState<Self::Settings>;
    type StateOperator = NoOperator<Self::State>;
    type Message = NoMessage;
}

#[async_trait::async_trait]
impl ServiceCore for DisseminateService {
    fn init(service_state: ServiceStateHandle<Self>) -> Result<Self, DynError> {
        Ok(Self { service_state })
    }

    async fn run(self) -> Result<(), DynError> {
        let Self { service_state } = self;
        let Settings {
            payload,
            timeout,
            da_protocol,
            status_updates,
            node_addr,
            output,
            private_key,
        } = service_state.settings_reader.get_updated_settings();

        let da_protocol: FullReplication<_> = da_protocol.try_into()?;

        let network_relay = service_state
            .overwatch_handle
            .relay::<NetworkService<Libp2p>>()
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        while let Some(data) = payload.lock().await.recv().await {
            match tokio::time::timeout(
                timeout,
                disseminate_and_wait(
                    da_protocol.clone(),
                    data,
                    Libp2pAdapter::new(network_relay.clone()).await,
                    status_updates.clone(),
                    node_addr.as_ref(),
                    output.as_ref(),
                    &private_key,
                ),
            )
            .await
            {
                Err(_) => {
                    tracing::error!("Timeout reached, check the logs for additional details");
                    let _ = status_updates.send(Status::Err("Timeout reached".into()));
                }
                Ok(Err(e)) => {
                    tracing::error!(
                        "Could not disseminate blob, check logs for additional details"
                    );
                    let _ = status_updates.send(Status::Err(e));
                }
                _ => {}
            }
        }

        service_state.overwatch_handle.shutdown().await;
        Ok(())
    }
}

// This format is for clap args convenience, I could not
// find a way to use enums directly without having to implement
// parsing by hand.
// The `settings` field will hold the settings for all possible
// protocols, but only the one chosen will be used.
// We can enforce only sensible combinations of protocol/settings
// are specified by using special clap directives
#[derive(Clone, Debug, Args)]
pub struct DaProtocolChoice {
    #[clap(long, default_value = "full-replication")]
    pub da_protocol: Protocol,
    #[clap(flatten)]
    pub settings: ProtocolSettings,
}

impl TryFrom<DaProtocolChoice> for FullReplication<AbsoluteNumber<Attestation, Certificate>> {
    type Error = &'static str;
    fn try_from(value: DaProtocolChoice) -> Result<Self, Self::Error> {
        match (value.da_protocol, value.settings) {
            (Protocol::FullReplication, ProtocolSettings { full_replication }) => {
                Ok(FullReplication::new(
                    full_replication.voter,
                    AbsoluteNumber::new(full_replication.num_attestations),
                ))
            }
        }
    }
}

#[derive(Clone, Debug, Args)]
pub struct ProtocolSettings {
    #[clap(flatten)]
    pub full_replication: FullReplicationSettings,
}

#[derive(Clone, Debug, ValueEnum)]
pub enum Protocol {
    FullReplication,
}

impl Default for FullReplicationSettings {
    fn default() -> Self {
        Self {
            voter: [0; 32],
            num_attestations: 1,
        }
    }
}

#[derive(Debug, Clone, Args)]
pub struct FullReplicationSettings {
    #[clap(long, value_parser = parse_key, default_value = "0000000000000000000000000000000000000000000000000000000000000000")]
    pub voter: Voter,
    #[clap(long, default_value = "1")]
    pub num_attestations: usize,
}

fn parse_key(s: &str) -> Result<Voter, Box<dyn Error + Send + Sync + 'static>> {
    Ok(<[u8; 32]>::from_hex(s)?)
}
