/// The dumbest possible backend for a chat protocol.
/// Just because running Doom on Nomos was too much work for a demo.
///
///
mod ui;

use crate::{
    api::consensus::get_headers_info,
    da::{
        disseminate::{
            DaProtocolChoice, DisseminateApp, DisseminateAppServiceSettings, Settings, Status,
        },
        retrieve::get_block_blobs,
    },
};
use clap::Args;
use full_replication::{
    AbsoluteNumber, Attestation, Certificate, FullReplication, Settings as DaSettings,
};
use futures::{stream, StreamExt};
use nomos_core::{da::DaProtocol, header::HeaderId, wire};
use nomos_log::{LoggerBackend, LoggerSettings, SharedWriter};
use nomos_network::backends::libp2p::Libp2p as NetworkBackend;
use nomos_network::NetworkService;
use overwatch_rs::{overwatch::OverwatchRunner, services::ServiceData};
use reqwest::Url;
use serde::{Deserialize, Serialize};
use std::{
    io,
    path::PathBuf,
    sync::{
        self,
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
// Limit the number of maximum in-flight requests
const MAX_BUFFERED_REQUESTS: usize = 20;

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
    /// Author for non interactive message formation
    #[clap(long, requires("message"))]
    pub author: Option<String>,
    /// Message for non interactive message formation
    #[clap(long, requires("author"))]
    pub message: Option<String>,
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
    logs: Arc<sync::Mutex<Vec<u8>>>,
    scroll_logs: u16,
}

impl NomosChat {
    pub fn run(&self) -> Result<(), Box<dyn std::error::Error>> {
        let network = serde_yaml::from_reader::<
            _,
            <NetworkService<NetworkBackend> as ServiceData>::Settings,
        >(std::fs::File::open(&self.network_config)?)?;
        let da_protocol = self.da_protocol.clone();

        let node_addr = Some(self.node.clone());

        let (payload_sender, payload_receiver) = tokio::sync::mpsc::unbounded_channel();
        let (status_sender, status_updates) = std::sync::mpsc::channel();

        let shared_writer = Arc::new(sync::Mutex::new(Vec::new()));
        let backend = SharedWriter::from_inner(shared_writer.clone());

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
                    logger: LoggerSettings {
                        backend: LoggerBackend::Writer(backend),
                        level: tracing::Level::INFO,
                        ..Default::default()
                    },
                },
                None,
            )
            .unwrap()
            .wait_finished()
        });

        if let Some(author) = self.author.as_ref() {
            let message = self
                .message
                .as_ref()
                .expect("Should be available if author is set");
            return run_once(author, message, payload_sender);
        }

        // setup terminal
        enable_raw_mode()?;
        let mut stdout = io::stdout();
        execute!(stdout, EnterAlternateScreen, EnableMouseCapture)?;
        let backend = CrosstermBackend::new(stdout);
        let mut terminal = Terminal::new(backend)?;

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
            logs: shared_writer,
            scroll_logs: 0,
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

fn run_once(
    author: &str,
    message: &str,
    payload_sender: UnboundedSender<Box<[u8]>>,
) -> Result<(), Box<dyn std::error::Error>> {
    payload_sender.send(
        wire::serialize(&ChatMessage {
            author: author.to_string(),
            message: message.to_string(),
            _nonce: rand::random(),
        })
        .unwrap()
        .into(),
    )?;

    Ok(())
}

fn run_app<B: Backend>(terminal: &mut Terminal<B>, mut app: App) {
    let (message_tx, message_rx) = std::sync::mpsc::channel();
    let node = app.node.clone();
    std::thread::spawn(move || check_for_messages(message_tx, node));
    loop {
        terminal.draw(|f| ui::ui(f, &app)).unwrap();

        if let Ok(update) = app.status_updates.try_recv() {
            if let Status::Done | Status::Err(_) = update {
                app.message_in_flight = false;
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
                                        _nonce: rand::random(),
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
                KeyCode::Left => {
                    app.scroll_logs = app.scroll_logs.saturating_sub(1);
                }
                KeyCode::Right => {
                    app.scroll_logs = app.scroll_logs.saturating_add(1);
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
    // Since DA will rightfully ignore duplicated messages, we need to add a nonce to make sure
    // every message is unique. This is randomly generated for simplicity.
    _nonce: u64,
}

#[tokio::main]
async fn check_for_messages(sender: Sender<Vec<ChatMessage>>, node: Url) {
    // Should ask for the genesis block to be more robust
    let mut last_tip = [0; 32].into();

    loop {
        if let Ok((new_tip, messages)) = fetch_new_messages(&last_tip, &node).await {
            sender.send(messages).expect("channel closed");
            last_tip = new_tip;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

// Process a single block's blobs and return chat messages
async fn process_block_blobs(
    node: Url,
    block_id: &HeaderId,
    da_settings: DaSettings,
) -> Result<Vec<ChatMessage>, Box<dyn std::error::Error>> {
    let blobs = get_block_blobs(&node, block_id).await?;

    // Note that number of attestations is ignored here since we only use the da protocol to
    // decode the blob data, not to validate the certificate
    let mut da_protocol =
        <FullReplication<AbsoluteNumber<Attestation, Certificate>> as DaProtocol>::new(da_settings);
    let mut messages = Vec::new();

    for blob in blobs {
        da_protocol.recv_blob(blob);
        let bytes = da_protocol.extract().unwrap();
        if let Ok(message) = wire::deserialize::<ChatMessage>(&bytes) {
            messages.push(message);
        }
    }

    Ok(messages)
}

// Fetch new messages since the last tip
async fn fetch_new_messages(
    last_tip: &HeaderId,
    node: &Url,
) -> Result<(HeaderId, Vec<ChatMessage>), Box<dyn std::error::Error>> {
    // By only specifying the 'to' parameter we get all the blocks since the last tip
    let mut new_blocks = get_headers_info(node, None, Some(*last_tip))
        .await?
        .into_iter()
        .collect::<Vec<_>>();

    // The first block is the most recent one.
    // Note that the 'to' is inclusive so the above request will always return at least one block
    // as long as the block exists (which is the case since it was returned by a previous call)
    let new_tip = new_blocks[0];
    // We already processed the last block so let's remove it
    new_blocks.pop();

    let da_settings = DaSettings {
        num_attestations: 1,
        voter: [0; 32],
    };

    let block_stream = stream::iter(new_blocks.iter().rev());
    let results: Vec<_> = block_stream
        .map(|block| {
            let node = node.clone();
            let da_settings = da_settings.clone();

            process_block_blobs(node, block, da_settings)
        })
        .buffered(MAX_BUFFERED_REQUESTS)
        .collect::<Vec<_>>()
        .await;

    let mut new_messages = Vec::new();
    for result in results {
        new_messages.extend(result?);
    }

    Ok((new_tip, new_messages))
}
