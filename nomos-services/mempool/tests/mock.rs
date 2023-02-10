use nomos_log::{Logger, LoggerSettings};
use nomos_network::{
    backends::mock::{Mock, MockBackendMessage, MockConfig, MockContentTopic, MockMessage},
    NetworkConfig, NetworkMsg, NetworkService,
};
use overwatch_derive::*;
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};

use nomos_mempool::{
    backend::mockpool::MockPool,
    network::adapters::mock::{MockAdapter, MOCK_CONTENT_TOPIC},
    MempoolMsg, MempoolService,
};

#[derive(Services)]
struct MockPoolNode {
    logging: ServiceHandle<Logger>,
    network: ServiceHandle<NetworkService<Mock>>,
    mockpool: ServiceHandle<MempoolService<MockAdapter<String>, MockPool<String, String>>>,
}

#[test]
fn test_mockmempool() {
    let exist = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
    let exist2 = exist.clone();

    let predefined_messages = vec![
        MockMessage {
            payload: "This is foo".to_string(),
            content_topic: MockContentTopic {
                application_name: "mock network",
                version: 0,
                content_topic_name: MOCK_CONTENT_TOPIC,
            },
            version: 0,
            timestamp: 0,
        },
        MockMessage {
            payload: "This is bar".to_string(),
            content_topic: MockContentTopic {
                application_name: "mock network",
                version: 0,
                content_topic_name: MOCK_CONTENT_TOPIC,
            },
            version: 0,
            timestamp: 0,
        },
    ];

    let exp_txns = predefined_messages
        .iter()
        .map(|msg| msg.payload.clone())
        .collect::<std::collections::HashSet<_>>();

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
        },
        None,
    )
    .map_err(|e| eprintln!("Error encountered: {}", e))
    .unwrap();

    let network = app.handle().relay::<NetworkService<Mock>>();
    let mempool = app
        .handle()
        .relay::<MempoolService<MockAdapter<String>, MockPool<String, String>>>();

    app.spawn(async move {
        let network_outbound = network.connect().await.unwrap();
        let mempool_outbound = mempool.connect().await.unwrap();

        // subscribe to the mock content topic
        network_outbound
            .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                topic: MOCK_CONTENT_TOPIC,
            }))
            .await
            .unwrap();

        // try to wait all txs to be stored in mempool
        loop {
            tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
            let (mtx, mrx) = tokio::sync::oneshot::channel();
            mempool_outbound
                .send(MempoolMsg::View {
                    ancestor_hint: BlockId::default(),
                    reply_channel: mtx,
                })
                .await
                .unwrap();

            let items = mrx
                .await
                .unwrap()
                .into_iter()
                .collect::<std::collections::HashSet<_>>();

            if items.len() == exp_txns.len() {
                assert_eq!(exp_txns, items);
                exist.store(true, std::sync::atomic::Ordering::SeqCst);
                break;
            }
        }
    });

    while !exist2.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_millis(200));
    }
}
