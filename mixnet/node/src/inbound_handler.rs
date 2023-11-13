use std::net::SocketAddr;

use mixnet_protocol::{Body, ProtocolError};
use nym_sphinx::{
    addressing::nodes::NymNodeRoutingAddress, Delay, DestinationAddressBytes, NodeAddressBytes,
    PrivateKey,
};
use sphinx_packet::{ProcessedPacket, SphinxPacket};
use tokio::{net::TcpStream, sync::mpsc};

use crate::{forward_scheduler::Packet, MixnetNodeConfig, Result};

#[derive(Clone)]
pub struct InboundHandler {
    config: MixnetNodeConfig,
    client_tx: mpsc::Sender<Body>,
    packet_tx: mpsc::UnboundedSender<Packet>,
}

impl InboundHandler {
    pub fn new(
        config: MixnetNodeConfig,
        client_tx: mpsc::Sender<Body>,
        packet_tx: mpsc::UnboundedSender<Packet>,
    ) -> Self {
        Self {
            config,
            client_tx,
            packet_tx,
        }
    }

    pub async fn handle_connection(&self, mut socket: TcpStream) -> Result<()> {
        loop {
            let body = Body::read(&mut socket).await?;
            let this = self.clone();
            tokio::spawn(async move {
                if let Err(e) = this.handle_body(body).await {
                    tracing::error!("failed to handle body: {e}");
                }
            });
        }
    }

    async fn handle_body(&self, body: Body) -> Result<()> {
        match body {
            Body::SphinxPacket(packet) => self.handle_sphinx_packet(packet).await,
            Body::FinalPayload(payload) => {
                self.forward_body_to_client_notifier(Body::FinalPayload(payload))
                    .await
            }
            _ => unreachable!(),
        }
    }

    async fn handle_sphinx_packet(&self, packet: Box<SphinxPacket>) -> Result<()> {
        match packet
            .process(&PrivateKey::from(self.config.private_key))
            .map_err(ProtocolError::InvalidSphinxPacket)?
        {
            ProcessedPacket::ForwardHop(packet, next_node_addr, delay) => {
                self.forward_packet_to_next_hop(Body::SphinxPacket(packet), next_node_addr, delay)
                    .await
            }
            ProcessedPacket::FinalHop(destination_addr, _, payload) => {
                self.forward_payload_to_destination(Body::FinalPayload(payload), destination_addr)
                    .await
            }
        }
    }

    async fn forward_body_to_client_notifier(&self, body: Body) -> Result<()> {
        // TODO: Decrypt the final payload using the private key, if it's encrypted

        // Do not wait when the channel is full or no receiver exists
        self.client_tx.try_send(body)?;
        Ok(())
    }

    async fn forward_packet_to_next_hop(
        &self,
        packet: Body,
        next_node_addr: NodeAddressBytes,
        delay: Delay,
    ) -> Result<()> {
        tracing::debug!("Delaying the packet for {delay:?}");
        tokio::time::sleep(delay.to_duration()).await;

        self.forward(packet, NymNodeRoutingAddress::try_from(next_node_addr)?)
            .await
    }

    async fn forward_payload_to_destination(
        &self,
        payload: Body,
        destination_addr: DestinationAddressBytes,
    ) -> Result<()> {
        tracing::debug!("Forwarding final payload to destination mixnode");

        self.forward(
            payload,
            NymNodeRoutingAddress::try_from_bytes(&destination_addr.as_bytes())?,
        )
        .await
    }

    async fn forward(&self, body: Body, to: NymNodeRoutingAddress) -> Result<()> {
        let addr = SocketAddr::from(to);

        self.packet_tx.send(Packet::new(addr, body))?;
        Ok(())
    }
}
