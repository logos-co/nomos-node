use nomos_core::{
    header::HeaderId,
    tx::mock::{MockTransaction, MockTxId},
};
use nomos_mempool::{
    backend::mockpool::{MockPool, MockSettings, RECOVERY_FILE_PATH},
    network::adapters::mock::{MockAdapter, MOCK_TX_CONTENT_TOPIC},
    MempoolMsg, TxMempoolService, TxMempoolSettings,
};
use nomos_network::{
    backends::mock::{Mock, MockBackendMessage, MockConfig, MockMessage},
    NetworkConfig, NetworkMsg, NetworkService,
};
use nomos_tracing_service::{Tracing, TracingSettings};
use overwatch_derive::*;
use overwatch_rs::{overwatch::OverwatchRunner, OpaqueServiceHandle};
use services_utils::{
    overwatch::{recovery::operators::RecoveryBackend, JsonFileBackend},
    traits::FromSettings,
};

type MockRecoveryBackend = JsonFileBackend<
    MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>,
    TxMempoolSettings<MockSettings, ()>,
>;
type MockMempoolService = TxMempoolService<
    MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>,
    MockAdapter,
    MockRecoveryBackend,
>;

#[derive(Services)]
struct MockPoolNode {
    logging: OpaqueServiceHandle<Tracing>,
    network: OpaqueServiceHandle<NetworkService<Mock>>,
    mockpool: OpaqueServiceHandle<MockMempoolService>,
}

// Run the provided closure, and then removes any file-based recovery mechanism that was run.
fn run_with_recovery_teardown(run: impl Fn()) {
    run();
    let _ = std::fs::remove_file(RECOVERY_FILE_PATH);
}

#[test]
fn test_mockmempool() {
    run_with_recovery_teardown(|| {
        let exist = std::sync::Arc::new(std::sync::atomic::AtomicBool::new(false));
        let exist2 = exist.clone();

        let predefined_messages = vec![
            MockMessage {
                payload: "This is foo".to_string(),
                content_topic: MOCK_TX_CONTENT_TOPIC,
                version: 0,
                timestamp: 0,
            },
            MockMessage {
                payload: "This is bar".to_string(),
                content_topic: MOCK_TX_CONTENT_TOPIC,
                version: 0,
                timestamp: 0,
            },
        ];

        let exp_txns = predefined_messages
            .iter()
            .cloned()
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
                mockpool: TxMempoolSettings {
                    pool: MockSettings::default(),
                    network_adapter: (),
                },
                logging: TracingSettings::default(),
            },
            None,
        )
        .map_err(|e| eprintln!("Error encountered: {e}"))
        .unwrap();

        let network = app.handle().relay::<NetworkService<Mock>>();
        let mempool = app.handle().relay::<MockMempoolService>();

        app.spawn(async move {
            let network_outbound = network.connect().await.unwrap();
            let mempool_outbound = mempool.connect().await.unwrap();

            // subscribe to the mock content topic
            network_outbound
                .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                    topic: MOCK_TX_CONTENT_TOPIC.content_topic_name.to_string(),
                }))
                .await
                .unwrap();

            // try to wait all txs to be stored in mempool
            loop {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                let (mtx, mrx) = tokio::sync::oneshot::channel();
                mempool_outbound
                    .send(MempoolMsg::View {
                        ancestor_hint: [0; 32].into(),
                        reply_channel: mtx,
                    })
                    .await
                    .unwrap();

                let items = mrx
                    .await
                    .unwrap()
                    .map(|msg| msg.message().clone())
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

        let recovery_backend = MockRecoveryBackend::from_settings(&TxMempoolSettings {
            pool: MockSettings::default(),
            network_adapter: (),
        });
        let recovered_state = recovery_backend
            .load_state()
            .expect("Should not fail to load the state.");
        assert_eq!(recovered_state.pending_items().len(), 2);
        assert_eq!(recovered_state.in_block_items().len(), 0);
        assert!(recovered_state.last_item_timestamp() > 0);
    });
}
