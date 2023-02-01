use nomos_network::{
    backends::mock::{
        EventKind, Mock, MockBackendMessage, MockConfig, MockContentTopic, MockMessage,
    },
    NetworkConfig, NetworkMsg, NetworkService,
};
use overwatch_derive::*;
use overwatch_rs::{overwatch::OverwatchRunner, services::handle::ServiceHandle};

use nomos_mempool::{
    backend::mockpool::MockPool, network::adapters::mock::MockAdapter, MempoolService,
};

#[derive(Services)]
struct MockPoolNode {
    network: ServiceHandle<NetworkService<Mock>>,
    mockpool: ServiceHandle<MempoolService<MockAdapter<String>, MockPool<String, String>>>,
}

fn main() {
    tracing_subscriber::fmt::fmt()
        .with_env_filter(std::env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .with_file(false)
        .init();

    let app = OverwatchRunner::<MockPoolNode>::run(
        MockPoolNodeServiceSettings {
            network: NetworkConfig {
                backend: MockConfig {
                    predefined_messages: vec![
                        MockMessage {
                            payload: "This is foo".to_string(),
                            content_topic: MockContentTopic {
                                application_name: "mock network",
                                version: 0,
                                content_topic_name: "foo",
                            },
                            version: 0,
                            timestamp: 0,
                        },
                        MockMessage {
                            payload: "This is bar".to_string(),
                            content_topic: MockContentTopic {
                                application_name: "mock network",
                                version: 0,
                                content_topic_name: "bar",
                            },
                            version: 0,
                            timestamp: 0,
                        },
                    ],
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
    app.spawn(async {
        let outbound = network.connect().await.unwrap();
        let (tx, rx) = tokio::sync::oneshot::channel();
        outbound
            .send(NetworkMsg::Subscribe {
                kind: EventKind::Message,
                sender: tx,
            })
            .await
            .unwrap();

        let _rx = rx.await.unwrap();
        outbound
            .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                topic: "foo",
            }))
            .await
            .unwrap();
        outbound
            .send(NetworkMsg::Process(MockBackendMessage::RelaySubscribe {
                topic: "bar",
            }))
            .await
            .unwrap();
    });
    app.wait_finished();
}
