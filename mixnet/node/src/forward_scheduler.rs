use std::collections::{hash_map::Entry, HashMap};
use std::net::SocketAddr;

use mixnet_protocol::Body;
use tokio::sync::mpsc;

use crate::{forwarder::Forwarder, MixnetNodeConfig};

/// [`ForwardScheduler`] receives all packets processed by [`InboundHandler`]s,
/// and tosses them to corresponding [`Forwarder`]s.
///
/// Because [`ForwardScheduler`] is a single component where all packets are gathered to,
/// it must be as light as possible.
pub struct ForwardScheduler {
    config: MixnetNodeConfig,
    rx: mpsc::UnboundedReceiver<Packet>,
    forwarders: HashMap<SocketAddr, Forwarder>,
}

impl ForwardScheduler {
    pub fn new(rx: mpsc::UnboundedReceiver<Packet>, config: MixnetNodeConfig) -> Self {
        Self {
            config,
            rx,
            forwarders: HashMap::with_capacity(config.connection_pool_size),
        }
    }

    pub async fn run(mut self) {
        loop {
            tokio::select! {
                packet = self.rx.recv() => {
                    if let Some(packet) = packet {
                        self.schedule(packet, self.config).await;
                    } else {
                        unreachable!("Packet channel should not be closed, because PacketForwarder is also holding the send half");
                    }
                },
                _ = tokio::signal::ctrl_c() => {
                    tracing::info!("Shutting down packet forwarder task...");
                    return;
                }
            }
        }
    }

    async fn schedule(&mut self, packet: Packet, config: MixnetNodeConfig) {
        if let Entry::Vacant(entry) = self.forwarders.entry(packet.target) {
            entry.insert(Forwarder::new(packet.target, config));
        }

        let forwarder = self.forwarders.get_mut(&packet.target).unwrap();
        forwarder.schedule(packet.body);
    }
}

pub struct Packet {
    target: SocketAddr,
    body: Body,
}

impl Packet {
    pub fn new(target: SocketAddr, body: Body) -> Self {
        Self { target, body }
    }
}
