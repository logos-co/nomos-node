use nomos_core::block::BlockId;
use nomos_network::{
    backends::mock::{
        EventKind, Mock, MockBackendMessage, MockConfig, MockContentTopic, MockMessage,
    },
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
    network: ServiceHandle<NetworkService<Mock>>,
    mockpool: ServiceHandle<MempoolService<MockAdapter<String>, MockPool<String, String>>>,
}

#[test]
fn test_mockmempool() {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

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

    let t_predefined_messages = predefined_messages.clone();

    let app = OverwatchRunner::<MockPoolNode>::run(
        MockPoolNodeServiceSettings {
            network: NetworkConfig {
                backend: MockConfig {
                    predefined_messages,
                    duration: tokio::time::Duration::from_secs(1),
                    seed: 0,
                    version: 1,
                    weights: None,
                },
            },
            mockpool: (),
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
        let outbound = network.connect().await.unwrap();
        let mempool_outbound = mempool.connect().await.unwrap();

        let (tx, rx) = tokio::sync::oneshot::channel();
        // send subscribe message to the network service
        outbound
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender: tx,
            })
            .await
            .unwrap();

        let _rx = rx.await.unwrap();
        // subscribe to the mock content topic
        outbound
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
                    ancestor_hint: BlockId,
                    tx: mtx,
                })
                .await
                .unwrap();
            let items = mrx
                .await
                .unwrap()
                .into_iter()
                .collect::<std::collections::HashSet<_>>();
            if items.len() == t_predefined_messages.len() {
                for msg in t_predefined_messages {
                    assert!(items.contains(&msg.payload));
                }
                exist.store(true, std::sync::atomic::Ordering::SeqCst);
                break;
            }
        }
    });

    while !exist2.load(std::sync::atomic::Ordering::SeqCst) {
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
}
