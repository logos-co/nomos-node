use std::time::Duration;

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures::StreamExt;
use mixnet_client::{MessageStream, MixnetClient, MixnetClientConfig, MixnetClientMode};
use rand::{rngs::OsRng, Rng, RngCore};
use tests::MixNode;
use tokio::time::Instant;

pub fn mixnet(c: &mut Criterion) {
    c.bench_function("mixnet", |b| {
        b.to_async(tokio::runtime::Runtime::new().unwrap())
            .iter_custom(|iters| async move {
                let (_mixnodes, mut sender_client, mut destination_stream, msg) =
                    setup(100 * 1024).await;

                let start = Instant::now();
                for _ in 0..iters {
                    black_box(
                        send_receive_message(&msg, &mut sender_client, &mut destination_stream)
                            .await,
                    );
                }
                start.elapsed()
            })
    });
}

async fn setup(msg_size: usize) -> (Vec<MixNode>, MixnetClient<OsRng>, MessageStream, Vec<u8>) {
    let (mixnodes, node_configs, topology) = MixNode::spawn_nodes(3).await;

    let sender_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::Sender,
            topology: topology.clone(),
            connection_pool_size: 255,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
        },
        OsRng,
    );
    let destination_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::SenderReceiver(
                // Connect with the MixnetNode in the exit layer
                // According to the current implementation,
                // one of mixnodes the exit layer always will be selected as a destination.
                node_configs.last().unwrap().client_listen_address,
            ),
            topology,
            connection_pool_size: 255,
            max_retries: 3,
            retry_delay: Duration::from_secs(5),
        },
        OsRng,
    );
    let destination_stream = destination_client.run().await.unwrap();

    let mut msg = vec![0u8; msg_size];
    rand::thread_rng().fill_bytes(&mut msg);

    (mixnodes, sender_client, destination_stream, msg)
}

async fn send_receive_message<R: Rng>(
    msg: &[u8],
    sender_client: &mut MixnetClient<R>,
    destination_stream: &mut MessageStream,
) {
    let res = sender_client.send(msg.to_vec(), Duration::ZERO);
    assert!(res.is_ok());

    let received = destination_stream.next().await.unwrap().unwrap();
    assert_eq!(msg, received.as_slice());
}

criterion_group!(
  name = benches;
  config = Criterion::default().sample_size(10).measurement_time(Duration::from_secs(180));
  targets = mixnet
);
criterion_main!(benches);
