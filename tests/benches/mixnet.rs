use std::time::{Duration, Instant};

use criterion::{black_box, criterion_group, criterion_main, Criterion};
use futures::{Stream, StreamExt};
use mixnet_client::{MixnetClient, MixnetClientConfig, MixnetClientError, MixnetClientMode};
use mixnet_topology::MixnetTopology;
use rand::{rngs::OsRng, RngCore};

async fn test_one_message(
    msg: &[u8],
    topology: MixnetTopology,
    mut destination_stream: impl Stream<Item = Result<Vec<u8>, MixnetClientError>> + Send + Unpin,
) {
    let mut sender_client = MixnetClient::new(
        MixnetClientConfig {
            mode: MixnetClientMode::Sender,
            topology: topology.clone(),
        },
        OsRng,
    );

    let res = sender_client.send(msg.to_vec());
    assert!(res.is_ok());

    let received = destination_stream.next().await.unwrap().unwrap();
    assert_eq!(msg, received.as_slice());
}

fn bench_one_message(c: &mut Criterion) {
    const MESSAGE_SIZE: &[usize] = &[250, 500, 1000];
    for size in MESSAGE_SIZE {
        c.bench_function(&format!("one message {size}"), |b| {
            b.to_async(tokio::runtime::Runtime::new().unwrap())
                .iter_custom(|iters| async move {
                    let mut msg = vec![0u8; *size];
                    rand::thread_rng().fill_bytes(&mut msg);
                    let mut total_duration = Duration::ZERO;
                    for _ in 0..iters {
                        let (topology, destination_stream) =
                            tests::run_nodes_and_destination_client().await;
                        let start = Instant::now();
                        black_box(test_one_message(&msg, topology, destination_stream).await);
                        total_duration += start.elapsed();
                    }
                    total_duration
                });
        });
    }
}

criterion_group!(
  name = benches;
  config = Criterion::default().sample_size(10);
  targets = bench_one_message
);

criterion_main!(benches);
