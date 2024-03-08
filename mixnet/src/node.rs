use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sphinx_packet::crypto::{PrivateKey, PRIVATE_KEY_SIZE};
use tokio::sync::mpsc;

use crate::{
    error::MixnetError,
    fragment::{Fragment, MessageReconstructor},
    packet::{Message, Packet, PacketBody},
    poisson::Poisson,
};

/// Mix node implementation that returns Sphinx packets which needs to be forwarded to next mix nodes,
/// or messages reconstructed from Sphinx packets delivered through all mix layers.
pub struct MixNode {
    output_rx: mpsc::UnboundedReceiver<Output>,
}

struct MixNodeRunner {
    _config: MixNodeConfig,
    encryption_private_key: PrivateKey,
    poisson: Poisson,
    packet_queue: mpsc::Receiver<PacketBody>,
    message_reconstructor: MessageReconstructor,
    output_tx: mpsc::UnboundedSender<Output>,
}

/// Mix node configuration
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct MixNodeConfig {
    /// Private key for decrypting Sphinx packets
    pub encryption_private_key: [u8; PRIVATE_KEY_SIZE],
    /// Poisson delay rate per minutes
    pub delay_rate_per_min: f64,
}

const PACKET_QUEUE_SIZE: usize = 256;

/// Queue for sending packets to [`MixNode`]
pub type PacketQueue = mpsc::Sender<PacketBody>;

impl MixNode {
    /// Creates a [`MixNode`] and a [`PacketQueue`].
    ///
    /// This returns [`MixnetError`] if the given `config` is invalid.
    pub fn new(config: MixNodeConfig) -> Result<(Self, PacketQueue), MixnetError> {
        let encryption_private_key = PrivateKey::from(config.encryption_private_key);
        let poisson = Poisson::new(config.delay_rate_per_min)?;
        let (packet_tx, packet_rx) = mpsc::channel(PACKET_QUEUE_SIZE);
        let (output_tx, output_rx) = mpsc::unbounded_channel();

        MixNodeRunner {
            _config: config,
            encryption_private_key,
            poisson,
            packet_queue: packet_rx,
            message_reconstructor: MessageReconstructor::new(),
            output_tx,
        }
        .run();

        Ok((Self { output_rx }, packet_tx))
    }

    /// Returns a next `[Output]` to be emitted, if it exists and the Poisson delay is done (if necessary).
    pub async fn next(&mut self) -> Option<Output> {
        self.output_rx.recv().await
    }
}

impl MixNodeRunner {
    fn run(mut self) {
        tokio::spawn(async move {
            loop {
                if let Some(packet) = self.packet_queue.recv().await {
                    if let Err(e) = self.process_packet(packet) {
                        tracing::error!("failed to process packet. skipping it: {e}");
                    }
                }
            }
        });
    }

    fn process_packet(&mut self, packet: PacketBody) -> Result<(), MixnetError> {
        match packet {
            PacketBody::SphinxPacket(packet) => self.process_sphinx_packet(packet.as_ref())?,
            PacketBody::Fragment(fragment) => self.process_fragment(fragment.as_ref())?,
        }
        Ok(())
    }

    fn process_sphinx_packet(&self, packet: &[u8]) -> Result<(), MixnetError> {
        let output = Output::Forward(PacketBody::process_sphinx_packet(
            packet,
            &self.encryption_private_key,
        )?);
        let delay = self.poisson.interval(&mut OsRng);
        let output_tx = self.output_tx.clone();
        tokio::spawn(async move {
            tokio::time::sleep(delay).await;
            // output_tx is always expected to be not closed/dropped.
            output_tx.send(output).unwrap();
        });
        Ok(())
    }

    fn process_fragment(&mut self, fragment: &[u8]) -> Result<(), MixnetError> {
        if let Some(msg) = self
            .message_reconstructor
            .add_and_reconstruct(Fragment::from_bytes(fragment)?)
        {
            match Message::from_bytes(&msg)? {
                Message::Real(msg) => {
                    let output = Output::ReconstructedMessage(msg.into_boxed_slice());
                    // output_tx is always expected to be not closed/dropped.
                    self.output_tx.send(output).unwrap();
                }
                Message::DropCover(_) => {
                    tracing::debug!("Drop cover message has been reconstructed. Dropping it...");
                }
            }
        }
        Ok(())
    }
}

/// Output that [`MixNode::next`] returns.
#[derive(Debug, PartialEq, Eq)]
pub enum Output {
    /// Packet to be forwarded to the next mix node
    Forward(Packet),
    /// Message reconstructed from [`Packet`]s
    ReconstructedMessage(Box<[u8]>),
}

#[cfg(test)]
mod tests {
    use std::net::{IpAddr, Ipv4Addr, SocketAddr};

    use sphinx_packet::crypto::PublicKey;

    use crate::{
        packet::Packet,
        topology::{tests::gen_entropy, MixNodeInfo, MixnetTopology},
    };

    use super::*;

    #[tokio::test]
    async fn mixnode() {
        let encryption_private_key = PrivateKey::new();
        let node_info = MixNodeInfo::new(
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(127, 0, 0, 1)), 1000u16).into(),
            *PublicKey::from(&encryption_private_key).as_bytes(),
        )
        .unwrap();

        let topology = MixnetTopology::new(
            (0..2).map(|_| node_info.clone()).collect(),
            2,
            1,
            gen_entropy(),
        )
        .unwrap();
        let (mut mixnode, packet_queue) = MixNode::new(MixNodeConfig {
            encryption_private_key: encryption_private_key.to_bytes(),
            delay_rate_per_min: 60.0,
        })
        .unwrap();

        let msg = "hello".as_bytes().to_vec();
        let packets = Packet::build_real(msg.clone(), &topology).unwrap();
        let num_packets = packets.len();

        for packet in packets.into_iter() {
            packet_queue.send(packet.body()).await.unwrap();
        }

        for _ in 0..num_packets {
            match mixnode.next().await.unwrap() {
                Output::Forward(packet_to) => {
                    packet_queue.send(packet_to.body()).await.unwrap();
                }
                Output::ReconstructedMessage(_) => unreachable!(),
            }
        }

        assert_eq!(
            Output::ReconstructedMessage(msg.into_boxed_slice()),
            mixnode.next().await.unwrap()
        );
    }
}
