use nomos_consensus::{
    messages::VoteMsg,
    mock::{MockAdapter as ConsensusMockAdapter, MOCK_BLOCK_CONTENT_TOPIC},
    overlay::flat::Flat,
    CarnotConsensus, CarnotSettings, NodeId,
};
use nomos_core::{
    block::BlockHeader,
    fountain::mock::MockFountain,
    tx::mock::{MockTransaction, MockTxId},
    vote::mock::{MockTally, MockTallySettings, MockVote},
};
use nomos_log::{Logger, LoggerSettings};
use nomos_network::{
    backends::mock::{Mock, MockConfig, MockMessage},
    NetworkConfig, NetworkService,
};

use overwatch_derive::*;
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};

use nomos_mempool::{
    backend::mockpool::MockPool,
    network::adapters::mock::{MockAdapter, MOCK_TX_CONTENT_TOPIC},
    MempoolMsg, MempoolService,
};
use std::{
    sync::mpsc::{channel, Receiver, Sender},
    time::Duration,
};

#[derive(Services)]
struct MockPoolNode {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Mock>>,
    mockpool: ServiceHandle<MempoolService<MockAdapter, MockPool<MockTxId, MockTransaction>>>,
    #[allow(clippy::type_complexity)]
    consensus: ServiceHandle<
        CarnotConsensus<
            ConsensusMockAdapter,
            MockPool<MockTxId, MockTransaction>,
            MockAdapter,
            MockFountain,
            MockTally,
            Flat<MockTxId>,
        >,
    >,
}

#[test]
fn test_carnot() {
    // let x = VoteMsg::from_bytes();
    let mock_vote_msg = VoteMsg {
        source: NodeId::default(),
        vote: MockVote::new(0),
    };
    let predefined_messages = vec![
        MockMessage {
            payload: String::from_utf8_lossy(&[0; 32]).to_string(),
            content_topic: MOCK_BLOCK_CONTENT_TOPIC,
            version: 0,
            timestamp: 0,
        },
        MockMessage {
            payload: String::from_utf8_lossy(&mock_vote_msg.as_bytes()).to_string(),
            content_topic: MOCK_TX_CONTENT_TOPIC,
            version: 0,
            timestamp: 0,
        },
    ];

    let expected = vec![MockTransaction::from(MockMessage {
        payload: String::from_utf8_lossy(&[0; 32]).to_string(),
        content_topic: MOCK_TX_CONTENT_TOPIC,
        version: 0,
        timestamp: 0,
    })];

    let app = OverwatchRunner::<MockPoolNode>::run(
        MockPoolNodeServiceSettings {
            network: NetworkConfig {
                backend: MockConfig {
                    predefined_messages,
                    duration: tokio::time::Duration::from_millis(100),
                    seed: 0,
                    version: 1,
                    weights: None,
                },
            },
            mockpool: (),
            logging: LoggerSettings::default(),
            consensus: CarnotSettings::new(
                Default::default(),
                (),
                MockTallySettings { threshold: 0 },
            ),
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap();

    let mempool = app
        .handle()
        .relay::<MempoolService<MockAdapter, MockPool<MockTxId, MockTransaction>>>();

    let (stop_tx, stop_rx): (Sender<()>, Receiver<()>) = channel();
    app.spawn(async move {
        let mempool_outbound = mempool.connect().await.unwrap();
        loop {
            // send block transactions message to mempool, and check if the previous transaction has been in the in_block_txs
            let (mtx, mrx) = tokio::sync::oneshot::channel();
            mempool_outbound
                .send(MempoolMsg::BlockTransaction {
                    block: BlockHeader::new().id(),
                    reply_channel: mtx,
                })
                .await
                .unwrap();

            let items = match mrx.await.unwrap() {
                Some(items) => items.into_iter().collect(),
                None => Vec::new(),
            };

            // we only send two transaction messages, so we expect two transactions in the in_block_txs
            if items.len() != expected.len() {
                continue;
            }
            assert_eq!(items, expected);
            stop_tx.send(()).unwrap();
        }
    });

    stop_rx.recv_timeout(Duration::from_secs(3)).unwrap();
}
