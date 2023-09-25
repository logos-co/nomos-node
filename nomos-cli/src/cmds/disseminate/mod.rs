use clap::{Args, ValueEnum};
use crossterm::execute;
use full_replication::{AbsoluteNumber, FullReplication};
use futures::StreamExt;
use nomos_core::da::DaProtocol;
use nomos_da::network::{adapters::libp2p::Libp2pAdapter, NetworkAdapter};
use nomos_network::{backends::libp2p::Libp2p, NetworkService};
use overwatch_derive::*;
use overwatch_rs::{
    overwatch::OverwatchRunner,
    services::{
        handle::{ServiceHandle, ServiceStateHandle},
        relay::NoMessage,
        state::*,
        ServiceCore, ServiceData, ServiceId,
    },
    DynError,
};
use std::{path::PathBuf, time::Duration};

#[derive(Args, Debug)]
pub struct Disseminate {
    // TODO: accept bytes
    #[clap(short, long)]
    data: String,
    /// Path to the network config file
    #[clap(short, long)]
    network_config: PathBuf,
    /// The data availability protocol to use. Defaults to full replication.
    #[clap(flatten)]
    da_protocol: DaProtocolChoice,
    /// Timeout in seconds. Defaults to 120 seconds.
    #[clap(short, long, default_value = "120")]
    timeout: u64,
}

#[derive(Debug, Clone, Args)]
pub struct FullReplicationSettings {
    #[clap(long, default_value = "1")]
    num_attestations: usize,
}

impl Disseminate {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        tracing::subscriber::set_global_default(tracing_subscriber::FmtSubscriber::new())
            .expect("setting tracing default failed");
        let network = serde_yaml::from_reader::<
            _,
            <NetworkService<Libp2p> as ServiceData>::Settings,
        >(std::fs::File::open(&self.network_config)?)?;
        OverwatchRunner::<DisseminateApp>::run(
            DisseminateAppServiceSettings {
                network,
                send_blob: Settings {
                    bytes: self.data.clone().as_bytes().into(),
                    timeout: Duration::from_secs(self.timeout),
                    da_protocol: self.da_protocol.clone(),
                },
            },
            None,
        )
        .unwrap()
        .wait_finished();
        Ok(())
    }
}

// Write '✓' at the end of the previous line in terminal
fn terminal_cmd_done() {
    let go_to_previous_line = crossterm::cursor::MoveToPreviousLine(1);
    let go_to_end_of_line =
        crossterm::cursor::MoveToColumn(crossterm::terminal::size().unwrap().0 - 1);
    let write_done = crossterm::style::Print("✓");
    execute!(
        std::io::stdout(),
        go_to_previous_line,
        go_to_end_of_line,
        write_done
    )
    .unwrap()
}

async fn disseminate<D, B, N, A, C>(
    mut da: D,
    data: Box<[u8]>,
    adapter: N,
) -> Result<C, Box<dyn std::error::Error>>
where
    D: DaProtocol<Blob = B, Attestation = A, Certificate = C>,
    N: NetworkAdapter<Blob = B, Attestation = A> + Send + Sync,
{
    // 1) Building blob
    tracing::info!("Building blobs...");
    let blobs = da.encode(data);
    terminal_cmd_done();

    // 2) Send blob to network
    tracing::info!("Sending blobs to network...");
    futures::future::try_join_all(blobs.into_iter().map(|blob| adapter.send_blob(blob)))
        .await
        .map_err(|e| e as Box<dyn std::error::Error>)?;
    terminal_cmd_done();

    // 3) Collect attestations and create proof
    tracing::info!("Collecting attestations to create proof...");
    let mut attestations = adapter.attestation_stream().await;
    loop {
        da.recv_attestation(attestations.next().await.unwrap());

        if let Some(cert) = da.certify_dispersal() {
            terminal_cmd_done();
            return Ok(cert);
        }
    }
}

// To interact with the network service it's easier to just spawn
// an overwatch app
#[derive(Services)]
struct DisseminateApp {
    network: ServiceHandle<NetworkService<Libp2p>>,
    send_blob: ServiceHandle<DisseminateService>,
}

#[derive(Clone, Debug)]
pub struct Settings {
    bytes: Box<[u8]>,
    timeout: Duration,
    da_protocol: DaProtocolChoice,
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
        let settings = service_state.settings_reader.get_updated_settings();

        match settings.da_protocol {
            DaProtocolChoice {
                da_protocol: Protocol::FullReplication,
                settings:
                    ProtocolSettings {
                        full_replication: da_settings,
                    },
            } => {
                let network_relay = service_state
                    .overwatch_handle
                    .relay::<NetworkService<Libp2p>>()
                    .connect()
                    .await
                    .expect("Relay connection with NetworkService should succeed");

                let adapter = Libp2pAdapter::new(network_relay).await;
                let da = FullReplication::new(AbsoluteNumber::new(da_settings.num_attestations));

                if tokio::time::timeout(settings.timeout, disseminate(da, settings.bytes, adapter))
                    .await
                    .is_err()
                {
                    tracing::error!("Timeout reached, check the logs for additional details");
                }
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
pub(super) struct DaProtocolChoice {
    #[clap(long, default_value = "full-replication")]
    da_protocol: Protocol,
    #[clap(flatten)]
    settings: ProtocolSettings,
}

#[derive(Clone, Debug, Args)]
struct ProtocolSettings {
    #[clap(flatten)]
    full_replication: FullReplicationSettings,
}

#[derive(Clone, Debug, ValueEnum)]
pub(super) enum Protocol {
    FullReplication,
}

impl Default for FullReplicationSettings {
    fn default() -> Self {
        Self {
            num_attestations: 1,
        }
    }
}
