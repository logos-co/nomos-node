// std
use std::{
    path::PathBuf,
    sync::{mpsc::Sender, Arc},
    time::Duration,
};
// crates
use clap::Args;
use kzgrs_backend::{
    common::build_blob_id,
    dispersal::{BlobInfo, Metadata},
    encoder::EncodedData as KzgEncodedData,
};
use nomos_core::{
    da::{DaDispersal, DaEncoder},
    wire,
};
use nomos_da_network_service::NetworkService;
use nomos_log::Logger;
use overwatch_derive::*;
use overwatch_rs::{
    services::{
        handle::{ServiceHandle, ServiceStateHandle},
        relay::{NoMessage, OutboundRelay, Relay},
        state::*,
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};
use reqwest::Url;
use subnetworks_assignations::versions::v1::FillFromNodeList;
use tokio::sync::{mpsc::UnboundedReceiver, Mutex};
// internal
use super::network::{
    adapters::{libp2p::Libp2pExecutorDispersalAdapter, mock::MockExecutorDispersalAdapter},
    backend::ExecutorBackend,
};
use crate::api::mempool::send_blob_info;

type NetworkBackend = ExecutorBackend<FillFromNodeList>;

pub async fn disseminate_and_wait<E, D>(
    encoder: &E,
    disperal: &D,
    data: Box<[u8]>,
    metadata: Metadata,
    status_updates: Sender<Status>,
    node_addr: Option<&Url>,
    output: Option<&PathBuf>,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>>
where
    E: DaEncoder<EncodedData = KzgEncodedData>,
    D: DaDispersal<EncodedData = KzgEncodedData>,
    <E as nomos_core::da::DaEncoder>::Error: std::error::Error + Send + Sync + 'static,
    <D as nomos_core::da::DaDispersal>::Error: std::error::Error + Send + Sync + 'static,
{
    // 1) Building blob
    status_updates.send(Status::Encoding)?;
    let encoded_data = encoder.encode(&data).map_err(Box::new)?;
    let blob_hash = build_blob_id(
        &encoded_data.aggregated_column_commitment,
        &encoded_data.row_commitments,
    );

    // 2) Send blob to network
    status_updates.send(Status::Disseminating)?;
    disperal.disperse(encoded_data).await.map_err(Box::new)?;

    // 3) Build blob info.
    let blob_info = BlobInfo::new(blob_hash, metadata);

    if let Some(output) = output {
        status_updates.send(Status::SavingBlobInfo)?;
        std::fs::write(output, wire::serialize(&blob_info)?)?;
    }

    // 4) Send blob info to the mempool.
    if let Some(node) = node_addr {
        status_updates.send(Status::SendingBlobInfo)?;
        let res = send_blob_info(node, &blob_info).await?;

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
    SavingBlobInfo,
    SendingBlobInfo,
    Done,
    Err(Box<dyn std::error::Error + Send + Sync>),
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Encoding => write!(f, "Encoding message into blob(s)"),
            Self::Disseminating => write!(f, "Sending blob(s) to the network"),
            Self::WaitingAttestations => write!(f, "Waiting for attestations"),
            Self::CreatingCert => write!(f, "Creating certificate"),
            Self::SavingBlobInfo => write!(f, "Saving blob info to file"),
            Self::SendingBlobInfo => write!(f, "Sending blob info to node"),
            Self::Done => write!(f, ""),
            Self::Err(e) => write!(f, "Error: {e}"),
        }
    }
}

// To interact with the network service it's easier to just spawn
// an overwatch app
#[derive(Services)]
pub struct DisseminateApp {
    network: ServiceHandle<NetworkService<NetworkBackend>>,
    send_blob: ServiceHandle<DisseminateService>,
    logger: ServiceHandle<Logger>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    // This is wrapped in an Arc just to make the struct Clone
    pub payload: Arc<Mutex<UnboundedReceiver<Box<[u8]>>>>,
    pub timeout: Duration,
    pub kzgrs_settings: KzgrsSettings,
    pub metadata: Metadata,
    pub status_updates: Sender<Status>,
    pub node_addr: Option<Url>,
    pub output: Option<std::path::PathBuf>,
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
            kzgrs_settings,
            metadata,
            status_updates,
            node_addr,
            output,
        } = service_state.settings_reader.get_updated_settings();

        let network_relay: Relay<NetworkService<NetworkBackend>> =
            service_state.overwatch_handle.relay();
        let network_relay: OutboundRelay<_> = network_relay
            .connect()
            .await
            .expect("Relay connection with NetworkService should succeed");

        let params = kzgrs_backend::encoder::DaEncoderParams::new(
            kzgrs_settings.num_columns,
            kzgrs_settings.with_cache,
        );
        let da_encoder = kzgrs_backend::encoder::DaEncoder::new(params);
        let da_dispersal = Libp2pExecutorDispersalAdapter::new(network_relay);

        while let Some(data) = payload.lock().await.recv().await {
            match tokio::time::timeout(
                timeout,
                disseminate_and_wait(
                    &da_encoder,
                    &da_dispersal,
                    data,
                    metadata,
                    status_updates.clone(),
                    node_addr.as_ref(),
                    output.as_ref(),
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

#[derive(Debug, Clone, Args)]
pub struct KzgrsSettings {
    num_columns: usize,
    with_cache: bool,
}

impl Default for KzgrsSettings {
    fn default() -> Self {
        Self {
            num_columns: 4096,
            with_cache: true,
        }
    }
}
