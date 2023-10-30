/// The dumbest possible backend for a chat protocol.
/// Just because running Doom on Nomos was too much work for a demo.
///
///
mod ui;

use crate::da::{
    disseminate::{
        DaProtocolChoice, DisseminateApp, DisseminateAppServiceSettings, Settings, Status,
    },
    retrieve::get_block_blobs,
};
use clap::Args;
use full_replication::{
    AbsoluteNumber, Attestation, Certificate, FullReplication, Settings as DaSettings,
};
use nomos_consensus::CarnotInfo;
use nomos_core::{block::BlockId, da::DaProtocol, wire};
use nomos_network::{backends::libp2p::Libp2p, NetworkService};
use overwatch_rs::{overwatch::OverwatchRunner, services::ServiceData};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    collections::HashSet,
    io,
    path::PathBuf,
    sync::{
        mpsc::{Receiver, Sender},
        Arc,
    },
    time::{Duration, Instant},
};
use tokio::sync::{mpsc::UnboundedSender, Mutex};

use crossterm::{
    event::{self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode},
    execute,
    terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen},
};
use ratatui::{
    backend::{Backend, CrosstermBackend},
    Terminal,
};
use tui_input::{backend::crossterm::EventHandler, Input};

const DEFAULT_TIMEOUT: Duration = Duration::from_secs(120);

#[derive(Clone, Debug, Args)]
/// The almost-instant messaging protocol.
pub struct NomosChat {
    /// Path to the network config file
    #[clap(short, long)]
    pub network_config: PathBuf,
    /// The data availability protocol to use. Defaults to full replication.
    #[clap(flatten)]
    pub da_protocol: DaProtocolChoice,
    /// The node to connect to to fetch blocks and blobs
    #[clap(long)]
    pub node: Url,
}

pub struct App {
    input: Input,
    username: Option<String>,
    messages: Vec<ChatMessage>,
    message_status: Option<Status>,
    message_in_flight: bool,
    last_updated: Instant,
    payload_sender: UnboundedSender<Box<[u8]>>,
    status_updates: Receiver<Status>,
    node: Url,
}

impl NomosChat {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let network = serde_yaml::from_reader::<
            _,
            <NetworkService<Libp2p> as ServiceData>::Settings,
        >(std::fs::File::open(&self.network_config)?)?;
        let da_protocol = self.da_protocol.clone();
        // setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

        let node_addr = Some(self.node.clone());

        let (payload_sender, payload_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (status_sender, status_updates) = std::sync::mpsc::channel();

        std::thread::spawn(move || {
            OverwatchRunner::<DisseminateApp>::run(
                DisseminateAppServiceSettings {
                    network,
                    send_blob: Settings {
                        payload: Arc::new(Mutex::new(payload_receiver)),
                        timeout: DEFAULT_TIMEOUT,
                        da_protocol,
                        status_updates: status_sender,
                        node_addr,
                        output: None,
                    },
                },
                None,
            )
            .unwrap()
            .wait_finished()
        });

        let app = App {
            input: Input::default(),
            username: None,
            messages: Vec::new(),
            message_status: None,
            message_in_flight: false,
            last_updated: Instant::now(),
            payload_sender,
            status_updates,
            node: self.node.clone(),
        };

        run_app(&mut terminal, app);

        // restore terminal
        disable_raw_mode()?;
        execute!(
            terminal.backend_mut(),
            LeaveAlternateScreen,
            DisableMouseCapture
        )?;
        terminal.show_cursor()?;

        Ok(())
    }
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) {
    let (message_tx, message_rx) = std::sync::mpsc::channel();
    let node = app.node.clone();
    std::thread::spawn(move || check_for_messages(message_tx, node));
    loop {
        terminal.draw(|f| ui::ui(f, &app)).unwrap();

        if let Ok(update) = app.status_updates.try_recv() {
            if let Status::Done = update {
                app.message_in_flight = false;
                app.message_status = None;
            }
            app.message_status = Some(update);
            app.last_updated = Instant::now();
        }

        if let Ok(messages) = message_rx.try_recv() {
            app.messages.extend(messages);
        }

        // Do not block rendering if there's no user input available
        if !event::poll(Duration::from_millis(100)).unwrap() {
            continue;
        }

        if let Event::Key(key) = event::read().unwrap() {
            match key.code {
                KeyCode::Enter => {
                    if app.username.is_none() {
                        app.username = Some(app.input.value().into());
                    } else {
                        // Do not allow more than one message in flight at a time for simplicity
                        if !app.message_in_flight && !app.input.value().is_empty() {
                            app.message_in_flight = true;
                            app.payload_sender
                                .send(
                                    wire::serialize(&ChatMessage {
                                        author: app.username.clone().unwrap(),
                                        message: app.input.value().into(),
                                    })
                                    .unwrap()
                                    .into(),
                                )
                                .unwrap();
                        }
                    }
                    app.input.reset();
                }
                KeyCode::Esc => {
                    return;
                }
                _ => {
                    if !app.message_in_flight {
                        app.input.handle_event(&Event::Key(key));
                    }
                }
            }
        }
    }
}

#[derive(Serialize, Deserialize, Debug)]
struct ChatMessage {
    author: String,
    message: String,
}

#[tokio::main]
async fn check_for_messages(sender: Sender<Vec<ChatMessage>>, node: Url) {
    let mut old_blocks = HashSet::new();

    loop {
        if let Ok(messages) = fetch_new_messages(&mut old_blocks, node.clone()).await {
            sender.send(messages).expect("channel closed");
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn fetch_new_messages(
    old_blocks: &mut HashSet<BlockId>,
    node: Url,
) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error>> {
    const NODE_CARNOT_INFO_PATH: &str = "carnot/info";

    let mut new_messages = Vec::new();

    let info = reqwest::get(node.join(NODE_CARNOT_INFO_PATH).unwrap())
        .await?
        .json::<CarnotInfo>()
        .await?;

    let new_blocks = info
        .committed_blocks
        .into_iter()
        .filter(|id| !old_blocks.contains(id) && id != &BlockId::zeros())
        .collect::<Vec<_>>();

    // note that number of attestations is ignored here since we only use the da protocol to
    // decode the blob data, not to validate the certificate
    let mut da_protocol =
        <FullReplication<AbsoluteNumber<Attestation, Certificate>> as DaProtocol>::new(
            DaSettings {
                num_attestations: 1,
            },
        );

    for block in new_blocks {
        let blobs = get_block_blobs(node.clone(), block).await?;
        for blob in blobs {
            da_protocol.recv_blob(blob);
            // full replication only needs one blob to decode the data, so the unwrap is safe
            let bytes = da_protocol.extract().unwrap();
            if let Ok(message) = wire::deserialize::<ChatMessage>(&bytes) {
                new_messages.push(message);
            }
        }
        old_blocks.insert(block);
    }

    Ok(new_messages)
}
