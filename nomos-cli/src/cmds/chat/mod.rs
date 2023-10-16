/// The dumbest possible backend for a chat protocol.
/// Just because running Doom on Nomos was too much work for a demo.
///
///
mod ui;

use crate::da::{
    DaProtocolChoice, DisseminateApp, DisseminateAppServiceSettings, Settings, Status,
};
use clap::Args;
use nomos_consensus::CarnotInfo;
use nomos_core::{da::DaProtocol, wire};
use nomos_network::{backends::libp2p::Libp2p, NetworkService};
use overwatch_rs::{overwatch::OverwatchRunner, services::ServiceData};
use reqwest::{blocking::Client, Url};
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
                        node_addr: None,
                        output: None,
                        wait_for_inclusion: false,
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
    std::thread::spawn(move || check_for_messages(message_tx));
    loop {
        terminal.draw(|f| ui::ui(f, &app)).unwrap();

        if let Ok(update) = app.status_updates.try_recv() {
            if let Status::Done = update {
                app.message_in_flight = false;
            }
            app.message_status = Some(update);
            app.last_updated = Instant::now();
        }

        if let Ok(message) = message_rx.try_recv() {
            app.messages.push(message);
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
                        //
                        if !app.message_in_flight && !app.input.value().is_empty() {
                            app.message_in_flight = true;
                            app.payload_sender
                                .send(app.input.value().as_bytes().into())
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

#[derive(Serialize, Deserialize)]
struct ChatMessage {
    author: String,
    message: String,
}

fn check_for_messages<D: DaProtocol>(da: D, sender: Sender<ChatMessage>, node: Url) {
    const NODE_CARNOT_INFO_PATH: &str = "consensus/info";
    const NODE_GET_BLOBS_PATH: &str = "da/blobs";

    let seen = HashSet::new();
    let client = Client::new();
    loop {
        std::thread::sleep(Duration::from_millis(500));
        let Ok(info) = client
            .get(node.join(NODE_CARNOT_INFO_PATH).unwrap())
            .send()
            .and_then(|resp| resp.json::<CarnotInfo>())
        else {
            // TODO: handle error
            continue;
        };

        let new_blocks = info
            .committed_blocks
            .into_iter()
            .filter(|id| !seen.contains(id))
            .collect::<Vec<_>>();

        seen.extend(new_blocks);

        // get blob

        // get blob contents

        let new_blobs = new_blocks
            .iter()
            .flat_map(|b| b.blobs.iter())
            .collect::<Vec<_>>();

        for blob in new_blobs {
            let blob = client
                .get(node.join(NODE_GET_BLOBS_PATH).unwrap())
                .blocking_send();

            d.recv_blob(b);
            if let Ok(msg) = wire::deserialize(d.extract()) {
                sender.send(msg).unwrap();
            }
        }
    }
}
