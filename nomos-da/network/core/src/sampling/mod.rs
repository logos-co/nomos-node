pub mod behaviour;

#[cfg(test)]
mod test {
    use crate::sampling::behaviour::{SamplingBehaviour, SamplingEvent};
    use crate::test_utils::AllNeighbours;
    use crate::SubnetworkId;
    use futures::StreamExt;
    use libp2p::identity::Keypair;
    use libp2p::swarm::SwarmEvent;
    use libp2p::{quic, Multiaddr, PeerId, Swarm, SwarmBuilder};
    use log::debug;
    use nomos_da_messages::sampling::{SampleReq, SampleRes};
    use std::time::Duration;
    use subnetworks_assignations::MembershipHandler;
    use tracing_subscriber::fmt::TestWriter;
    use tracing_subscriber::EnvFilter;

    pub fn sampling_swarm(
        key: Keypair,
        membership: impl MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static,
    ) -> Swarm<
        SamplingBehaviour<impl MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static>,
    > {
        SwarmBuilder::with_existing_identity(key)
            .with_tokio()
            .with_other_transport(|key| quic::tokio::Transport::new(quic::Config::new(key)))
            .unwrap()
            .with_behaviour(|_key| SamplingBehaviour::new(membership))
            .unwrap()
            .with_swarm_config(|cfg| {
                cfg.with_idle_connection_timeout(Duration::from_secs(u64::MAX))
            })
            .build()
    }
    #[tokio::test]
    async fn test_sampling_two_peers() {
        let _ = tracing_subscriber::fmt()
            .with_env_filter(EnvFilter::from_default_env())
            .compact()
            .with_writer(TestWriter::default())
            .try_init();
        let k1 = Keypair::generate_ed25519();
        let k2 = Keypair::generate_ed25519();
        let neighbours = AllNeighbours {
            neighbours: [
                PeerId::from_public_key(&k1.public()),
                PeerId::from_public_key(&k2.public()),
            ]
            .into_iter()
            .collect(),
        };

        let p1_address = "/ip4/127.0.0.1/udp/5080/quic-v1"
            .parse::<Multiaddr>()
            .unwrap()
            .with_p2p(PeerId::from_public_key(&k1.public()))
            .unwrap();
        let p2_address = "/ip4/127.0.0.1/udp/5081/quic-v1"
            .parse::<Multiaddr>()
            .unwrap()
            .with_p2p(PeerId::from_public_key(&k2.public()))
            .unwrap();
        let mut p1 = sampling_swarm(k1.clone(), neighbours.clone());
        let mut p2 = sampling_swarm(k2.clone(), neighbours);

        let request_sender_1 = p1.behaviour().sample_request_channel();
        let request_sender_2 = p2.behaviour().sample_request_channel();
        const MSG_COUNT: usize = 10;
        async fn test_sampling_swarm(
            mut swarm: Swarm<
                SamplingBehaviour<
                    impl MembershipHandler<Id = PeerId, NetworkId = SubnetworkId> + 'static,
                >,
            >,
        ) -> (Vec<SampleReq>, Vec<[u8; 32]>) {
            let mut req = vec![];
            let mut res = vec![];
            loop {
                match swarm.next().await {
                    None => {}
                    Some(SwarmEvent::Behaviour(SamplingEvent::IncomingSample {
                        request_receiver,
                        response_sender,
                    })) => {
                        req.push(request_receiver.await.unwrap());
                        response_sender
                            .send(SampleRes { message_type: None })
                            .unwrap()
                    }
                    Some(SwarmEvent::Behaviour(SamplingEvent::SamplingSuccess {
                        blob_id, ..
                    })) => {
                        res.push(blob_id);
                    }
                    Some(event) => {
                        debug!("{event:?}");
                    }
                }
                if (req.len(), res.len()) == (MSG_COUNT, MSG_COUNT) {
                    break (req, res);
                }
            }
        }
        let _p1_address = p1_address.clone();
        let _p2_address = p2_address.clone();
        p1.add_peer_address(PeerId::from_public_key(&k2.public()), _p2_address.clone());
        p2.add_peer_address(PeerId::from_public_key(&k1.public()), _p1_address.clone());
        let t1 = tokio::spawn(async move {
            p1.listen_on(p1_address).unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
            p1.dial(_p2_address).unwrap();
            test_sampling_swarm(p1).await
        });
        let t2 = tokio::spawn(async move {
            p2.listen_on(p2_address).unwrap();
            tokio::time::sleep(Duration::from_secs(1)).await;
            p2.dial(_p1_address).unwrap();
            test_sampling_swarm(p2).await
        });
        tokio::time::sleep(Duration::from_secs(2)).await;
        for _ in 0..MSG_COUNT {
            request_sender_1.send((0, [0; 32])).unwrap();
            request_sender_2.send((0, [0; 32])).unwrap();
        }

        let (req1, res1) = t1.await.unwrap();
        let (req2, res2) = t2.await.unwrap();
        assert_eq!(req1, req2);
        assert_eq!(res1, res2);
    }
}
