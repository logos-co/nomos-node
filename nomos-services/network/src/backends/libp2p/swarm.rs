use std::{collections::HashMap, time::Duration};

use nomos_libp2p::{
    gossipsub,
    libp2p::{
        identify,
        kad::{self, PeerInfo, QueryId},
        swarm::ConnectionId,
    },
    BehaviourEvent, Multiaddr, PeerId, Protocol, Swarm, SwarmEvent, KAD_PROTOCOL_NAME,
};
use tokio::sync::{broadcast, mpsc, oneshot};
use tokio_stream::StreamExt;

use super::{
    command::{Command, Dial, Topic},
    Event, Libp2pConfig,
};
use crate::backends::libp2p::Libp2pInfo;

pub struct SwarmHandler {
    pub swarm: Swarm,
    pub pending_dials: HashMap<ConnectionId, Dial>,
    pub commands_tx: mpsc::Sender<Command>,
    pub commands_rx: mpsc::Receiver<Command>,
    pub events_tx: broadcast::Sender<Event>,

    pending_queries: HashMap<QueryId, oneshot::Sender<Vec<PeerInfo>>>,
}

macro_rules! log_error {
    ($e:expr) => {
        if let Err(e) = $e {
            tracing::error!("error while processing {}: {e:?}", stringify!($e));
        }
    };
}

// TODO: make this configurable
const BACKOFF: u64 = 5;
// TODO: make this configurable
const MAX_RETRY: usize = 3;

impl SwarmHandler {
    pub fn new(
        config: &Libp2pConfig,
        commands_tx: mpsc::Sender<Command>,
        commands_rx: mpsc::Receiver<Command>,
        events_tx: broadcast::Sender<Event>,
    ) -> Self {
        let swarm = Swarm::build(&config.inner).unwrap();

        // Keep the dialing history since swarm.connect doesn't return the result
        // synchronously
        let pending_dials = HashMap::<ConnectionId, Dial>::new();

        Self {
            swarm,
            pending_dials,
            commands_tx,
            commands_rx,
            events_tx,
            pending_queries: HashMap::new(),
        }
    }

    pub async fn run(&mut self, initial_peers: Vec<Multiaddr>) {
        let local_peer_id = *self.swarm.swarm().local_peer_id();
        let local_addr = self.swarm.swarm().listeners().next().cloned();

        // add local address to kademlia
        if let Some(addr) = local_addr {
            self.swarm
                .swarm_mut()
                .behaviour_mut()
                .kademlia_add_address(local_peer_id, addr);
        }

        for peer_addr in &initial_peers {
            if let Some(Protocol::P2p(peer_id_bytes)) = peer_addr.iter().last() {
                if let Ok(peer_id) = PeerId::from_multihash(peer_id_bytes.into()) {
                    self.swarm
                        .swarm_mut()
                        .behaviour_mut()
                        .kademlia_add_address(peer_id, peer_addr.clone());
                    tracing::debug!("Added peer to Kademlia: {} at {}", peer_id, peer_addr);
                } else {
                    tracing::warn!("Failed to parse peer ID from multiaddr: {}", peer_addr);
                }
            } else {
                tracing::warn!("Multiaddr doesn't contain peer ID: {}", peer_addr);
            }

            self.swarm.swarm_mut().behaviour_mut().bootstrap_kademlia();
        }

        for initial_peer in &initial_peers {
            let (tx, _) = oneshot::channel();
            let dial = Dial {
                addr: initial_peer.clone(),
                retry_count: 0,
                result_sender: tx,
            };
            Self::schedule_connect(dial, self.commands_tx.clone()).await;
        }

        loop {
            tokio::select! {
                Some(event) = self.swarm.next() => {
                    self.handle_event(event);
                }
                Some(command) = self.commands_rx.recv() => {
                    self.handle_command(command);
                }
            }
        }
    }

    #[expect(
        clippy::cognitive_complexity,
        clippy::too_many_lines,
        reason = "TODO: Address this at some point."
    )]
    fn handle_event(&mut self, event: SwarmEvent<BehaviourEvent>) {
        match event {
            SwarmEvent::Behaviour(BehaviourEvent::Gossipsub(gossipsub::Event::Message {
                propagation_source: peer_id,
                message_id: id,
                message,
            })) => {
                tracing::debug!("Got message with id: {id} from peer: {peer_id}");
                log_error!(self.events_tx.send(Event::Message(message)));
            }
            // identify events (used for peer discovery)
            SwarmEvent::Behaviour(BehaviourEvent::Identify(identify::Event::Received {
                peer_id,
                info,
                ..
            })) => {
                tracing::debug!(
                    "Identified peer {} with addresses {:?}",
                    peer_id,
                    info.listen_addrs
                );

                let behaviour = self.swarm.swarm_mut().behaviour_mut();

                if info.protocols.iter().any(|p| *p == KAD_PROTOCOL_NAME) {
                    // we need to add the peer to the kademlia routing table
                    // in order to enable peer discovery
                    for addr in &info.listen_addrs {
                        behaviour.kademlia_add_address(peer_id, addr.clone());
                    }
                }
            }

            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::GetClosestPeers(Ok(result)),
                    ..
                },
            )) => {
                if let Some(sender) = self.pending_queries.remove(&id) {
                    let _ = sender.send(result.peers);
                }
            }

            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::GetClosestPeers(Err(err)),
                    ..
                },
            )) => {
                tracing::warn!("Failed to find closest peers: {:?}", err);

                if let Some(sender) = self.pending_queries.remove(&id) {
                    let _ = sender.send(Vec::new());
                }
            }
            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(
                kad::Event::OutboundQueryProgressed {
                    id,
                    result: kad::QueryResult::Bootstrap(result),
                    ..
                },
            )) => match result {
                Ok(_) => {
                    tracing::debug!("Bootstrap successful. Query ID: {:?}", id);
                }
                Err(e) => {
                    tracing::warn!("Bootstrap failed: {:?}, Query ID: {:?}", e, id);
                }
            },

            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(kad::Event::RoutingUpdated {
                peer,
                addresses,
                old_peer,
                is_new_peer,
                bucket_range,
            })) => {
                tracing::debug!(
                    "Routing table updated: peer: {peer}, addresses: {addresses:?}, old_peer: {old_peer:?}, is_new_peer: {is_new_peer}, bucket_range: {bucket_range:?}"
                );
            }

            SwarmEvent::Behaviour(BehaviourEvent::Kademlia(event)) => {
                tracing::debug!("Kademlia event: {:?}", event);
            }

            SwarmEvent::ConnectionEstablished {
                peer_id,
                connection_id,
                endpoint,
                ..
            } => {
                tracing::debug!("connected to peer:{peer_id}, connection_id:{connection_id:?}");
                if endpoint.is_dialer() {
                    self.complete_connect(connection_id, peer_id);
                }
            }
            SwarmEvent::ConnectionClosed {
                peer_id,
                connection_id,
                cause,
                ..
            } => {
                tracing::debug!(
                    "connection closed from peer: {peer_id} {connection_id:?} due to {cause:?}"
                );
            }
            SwarmEvent::OutgoingConnectionError {
                peer_id,
                connection_id,
                error,
                ..
            } => {
                tracing::error!(
                    "Failed to connect to peer: {peer_id:?} {connection_id:?} due to: {error}"
                );
                self.retry_connect(connection_id);
            }
            _ => {}
        }
    }

    #[expect(
        clippy::cognitive_complexity,
        reason = "TODO: Address this at some point."
    )]
    fn handle_command(&mut self, command: Command) {
        match command {
            Command::Connect(dial) => {
                self.connect(dial);
            }
            Command::Broadcast { topic, message } => {
                self.broadcast_and_retry(topic, message, 0);
            }
            Command::Subscribe(topic) => {
                tracing::debug!("subscribing to topic: {topic}");
                log_error!(self.swarm.subscribe(&topic));
            }
            Command::Unsubscribe(topic) => {
                tracing::debug!("unsubscribing to topic: {topic}");
                self.swarm.unsubscribe(&topic);
            }
            Command::Info { reply } => {
                let swarm = self.swarm.swarm();
                let network_info = swarm.network_info();
                let counters = network_info.connection_counters();
                let info = Libp2pInfo {
                    listen_addresses: swarm.listeners().cloned().collect(),
                    n_peers: network_info.num_peers(),
                    n_connections: counters.num_connections(),
                    n_pending_connections: counters.num_pending(),
                };
                log_error!(reply.send(info));
            }
            Command::RetryBroadcast {
                topic,
                message,
                retry_count,
            } => {
                self.broadcast_and_retry(topic, message, retry_count);
            }

            // this is just an illustration what kind of commands we could expose to other services
            // it is not optimized and is just a proof of concept
            Command::GetClosestPeers { peer_id, reply } => {
                tracing::info!("Getting closest peers to: {peer_id}");
                let query_id = self.swarm.get_closest_peers(peer_id);
                tracing::info!("Query id: {query_id:?}");
                self.pending_queries.insert(query_id, reply);
            }
            Command::DumpRoutingTable { reply } => {
                let result = self
                    .swarm
                    .swarm_mut()
                    .behaviour_mut()
                    .kademlia_routing_table_dump();
                let _ = reply.send(result);
            }
        }
    }

    async fn schedule_connect(dial: Dial, commands_tx: mpsc::Sender<Command>) {
        commands_tx
            .send(Command::Connect(dial))
            .await
            .unwrap_or_else(|_| tracing::error!("could not schedule connect"));
    }

    fn connect(&mut self, dial: Dial) {
        tracing::debug!("Connecting to {}", dial.addr);

        match self.swarm.connect(&dial.addr) {
            Ok(connection_id) => {
                // Dialing has been scheduled. The result will be notified as a SwarmEvent.
                self.pending_dials.insert(connection_id, dial);
            }
            Err(e) => {
                if let Err(err) = dial.result_sender.send(Err(e)) {
                    tracing::warn!("failed to send the Err result of dialing: {err:?}");
                }
            }
        }
    }

    fn complete_connect(&mut self, connection_id: ConnectionId, peer_id: PeerId) {
        if let Some(dial) = self.pending_dials.remove(&connection_id) {
            if let Err(e) = dial.result_sender.send(Ok(peer_id)) {
                tracing::warn!("failed to send the Ok result of dialing: {e:?}");
            }
        }
    }

    // TODO: Consider a common retry module for all use cases
    fn retry_connect(&mut self, connection_id: ConnectionId) {
        if let Some(mut dial) = self.pending_dials.remove(&connection_id) {
            dial.retry_count += 1;
            if dial.retry_count > MAX_RETRY {
                tracing::debug!("Max retry({MAX_RETRY}) has been reached: {dial:?}");
                return;
            }

            let wait = Self::exp_backoff(dial.retry_count);
            tracing::debug!("Retry dialing in {wait:?}: {dial:?}");

            let commands_tx = self.commands_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(wait).await;
                Self::schedule_connect(dial, commands_tx).await;
            });
        }
    }

    #[expect(
        clippy::cognitive_complexity,
        reason = "TODO: Address this at some point."
    )]
    fn broadcast_and_retry(&mut self, topic: Topic, message: Box<[u8]>, retry_count: usize) {
        tracing::debug!("broadcasting message to topic: {topic}");

        match self.swarm.broadcast(&topic, message.to_vec()) {
            Ok(id) => {
                tracing::debug!("broadcasted message with id: {id} tp topic: {topic}");
                // self-notification because libp2p doesn't do it
                if self.swarm.is_subscribed(&topic) {
                    log_error!(self.events_tx.send(Event::Message(gossipsub::Message {
                        source: None,
                        data: message.into(),
                        sequence_number: None,
                        topic: Swarm::topic_hash(&topic),
                    })));
                }
            }
            Err(gossipsub::PublishError::InsufficientPeers) if retry_count < MAX_RETRY => {
                let wait = Self::exp_backoff(retry_count);
                tracing::error!("failed to broadcast message to topic due to insufficient peers, trying again in {wait:?}");

                let commands_tx = self.commands_tx.clone();
                tokio::spawn(async move {
                    tokio::time::sleep(wait).await;
                    commands_tx
                        .send(Command::RetryBroadcast {
                            topic,
                            message,
                            retry_count: retry_count + 1,
                        })
                        .await
                        .unwrap_or_else(|_| tracing::error!("could not schedule retry"));
                });
            }
            Err(e) => {
                tracing::error!("failed to broadcast message to topic: {topic} {e:?}");
            }
        }
    }

    const fn exp_backoff(retry: usize) -> Duration {
        std::time::Duration::from_secs(BACKOFF.pow(retry as u32))
    }
}

#[cfg(test)]
mod tests {
    use std::{
        net::{Ipv4Addr, TcpListener},
        sync::Once,
    };

    use rand::{thread_rng, Rng};
    use tracing_subscriber::EnvFilter;

    use super::*;

    static INIT: Once = Once::new();

    fn init_tracing() {
        let filter = EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info"));

        INIT.call_once(|| {
            tracing_subscriber::fmt().with_env_filter(filter).init();
        });
    }

    fn get_next_available_port(start_port: u16) -> Option<u16> {
        (start_port..10000).find(|&port| TcpListener::bind(("127.0.0.1", port)).is_ok())
        // Return None if no ports are available
    }

    fn create_swarm_config() -> nomos_libp2p::SwarmConfig {
        let random_port = thread_rng().gen_range(8000, 9000);
        let port = get_next_available_port(random_port).expect("No available ports found");

        nomos_libp2p::SwarmConfig {
            host: Ipv4Addr::new(127, 0, 0, 1),
            port,
            node_key: nomos_libp2p::ed25519::SecretKey::generate(),
            gossipsub_config: gossipsub::Config::default(),
        }
    }

    fn create_libp2p_config(initial_peers: Vec<Multiaddr>) -> Libp2pConfig {
        Libp2pConfig {
            inner: create_swarm_config(),
            initial_peers,
        }
    }

    const NODE_COUNT: usize = 10;

    #[tokio::test]
    async fn test_kademlia_bootstrap() {
        init_tracing();

        let mut handler_tasks = Vec::with_capacity(NODE_COUNT);
        let mut txs = Vec::new();

        // Create first node (bootstrap node)
        let (tx1, rx1) = mpsc::channel(10);
        txs.push(tx1.clone());

        let (events_tx1, _) = broadcast::channel(10);
        let config = create_libp2p_config(vec![]);
        let mut bootstrap_node = SwarmHandler::new(&config, tx1.clone(), rx1, events_tx1.clone());

        let bootstrap_node_peer_id = *bootstrap_node.swarm.swarm().local_peer_id();

        let task1 = tokio::spawn(async move {
            bootstrap_node.run(vec![]).await;
        });
        handler_tasks.push(task1);

        // Wait for bootstrap node to start
        tokio::time::sleep(Duration::from_secs(5)).await;

        let (info_tx, info_rx) = oneshot::channel();
        tx1.send(Command::Info { reply: info_tx })
            .await
            .expect("Failed to send info command");
        let bootstrap_info = info_rx.await.expect("Failed to get bootstrap node info");

        assert!(
            !bootstrap_info.listen_addresses.is_empty(),
            "Bootstrap node has no listening addresses"
        );

        tracing::info!(
            "Bootstrap node listening on: {:?}",
            bootstrap_info.listen_addresses
        );

        // Use the first listening address as the bootstrap address
        let bootstrap_addr = bootstrap_info.listen_addresses[0]
            .clone()
            .with(Protocol::P2p(bootstrap_node_peer_id));

        tracing::info!("Using bootstrap address: {}", bootstrap_addr);

        let bootstrap_addr = bootstrap_addr.clone();

        // Start additional nodes
        for i in 1..NODE_COUNT {
            let (tx, rx) = mpsc::channel(10);
            txs.push(tx.clone());
            let (events_tx, _) = broadcast::channel(10);

            // Each node connects to the bootstrap node
            let config = create_libp2p_config(vec![bootstrap_addr.clone()]);
            let mut handler = SwarmHandler::new(&config, tx.clone(), rx, events_tx);

            let peer_id = *handler.swarm.swarm().local_peer_id();
            tracing::info!("Starting node {} with peer ID: {}", i, peer_id);

            let bootstrap_addr = bootstrap_addr.clone();
            let task = tokio::spawn(async move {
                handler.run(vec![bootstrap_addr.clone()]).await;
            });

            handler_tasks.push(task);

            // Add small delay between node startups to avoid overloading
            tokio::time::sleep(Duration::from_millis(500)).await;
        }

        tracing::info!("Waiting for DHT to populate...");
        tokio::time::sleep(Duration::from_secs(20)).await;

        // Check routing tables of all nodes
        for tx in &txs {
            let (dump_tx, dump_rx) = oneshot::channel();
            tx.send(Command::DumpRoutingTable { reply: dump_tx })
                .await
                .expect("Failed to send dump command");

            let routing_table = dump_rx.await.expect("Failed to receive routing table dump");

            // we expect all nodes in routing table as number of nodes is less then K
            // (kademlia bucket size)
            assert!(
                routing_table.len() >= NODE_COUNT - 1,
                "Expected at least {} entries in routing table, got {}",
                NODE_COUNT - 1,
                routing_table.len()
            );
        }

        // Verify bootstrap node connections
        let (info_tx, info_rx) = oneshot::channel();
        tx1.send(Command::Info { reply: info_tx })
            .await
            .expect("Failed to send info command");
        let bootstrap_info = info_rx.await.expect("Failed to get bootstrap node info");

        assert!(
            bootstrap_info.n_peers > 0,
            "Bootstrap node has 0 peers after test"
        );

        let (closest_tx, closest_rx) = oneshot::channel();
        tx1.send(Command::GetClosestPeers {
            peer_id: bootstrap_node_peer_id,
            reply: closest_tx,
        })
        .await
        .expect("Failed to send get closest peers command");

        let closest_peers = closest_rx.await.expect("Failed to get closest peers");

        assert!(
            closest_peers.len() >= NODE_COUNT - 1,
            "Expected at least {} closest peers, got {}",
            NODE_COUNT - 1,
            closest_peers.len()
        );

        for task in handler_tasks {
            task.abort();
        }
    }
}
