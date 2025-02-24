use nomos_core::{
    header::HeaderId,
    tx::mock::{MockTransaction, MockTxId},
};
use nomos_mempool::{
    backend::mockpool::{MockPool, MockSettings},
    network::adapters::mock::{MockAdapter, MOCK_TX_CONTENT_TOPIC},
    MempoolMsg, TxMempoolService, TxMempoolSettings,
};
use nomos_network::{
    backends::mock::{Mock, MockBackendMessage, MockConfig, MockMessage},
    NetworkConfig, NetworkMsg, NetworkService,
};
use nomos_tracing_service::{Tracing, TracingSettings};
use overwatch_derive::*;
use overwatch_rs::services::ServiceData;
use overwatch_rs::{
    overwatch::{
        commands::{OverwatchCommand, ServiceLifeCycleCommand},
        OverwatchRunner,
    },
    services::life_cycle::LifecycleMessage,
    OpaqueServiceHandle,
};
use services_utils::{
    overwatch::{
        recovery::{
            backends::FileBackendSettings, operators::RecoveryBackend, JsonRecoverySerializer,
            RecoveryResult,
        },
        FileBackend,
    },
    traits::FromSettings,
};

type InnerFileBackend = FileBackend<
    MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>,
    TxMempoolSettings<MockSettings, ()>,
    JsonRecoverySerializer<MockPool<HeaderId, MockTransaction<MockMessage>, MockTxId>>,
>;

struct MockRecoveryBackend(InnerFileBackend, MockSettings);

impl RecoveryBackend for MockRecoveryBackend {
    type State = <InnerFileBackend as RecoveryBackend>::State;

    fn save_state(&self, state: &Self::State) -> RecoveryResult<()> {
        self.0.save_state(state)
    }

    fn load_state(&self) -> RecoveryResult<Self::State> {
        self.0.load_state()
    }
}

// Delete the recovery file when not needed anymore.
impl Drop for MockRecoveryBackend {
    fn drop(&mut self) {
        let _ = std::fs::remove_file(self.1.recovery_file());
    }
}

impl FromSettings for MockRecoveryBackend {
    type Settings = TxMempoolSettings<MockSettings, ()>;

    fn from_settings(settings: &Self::Settings) -> Self {
        Self(
            InnerFileBackend::from_settings(settings),
            MockSettings::default(),
        )
    }
}

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

#[test]
fn test_mockmempool() {
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
}
