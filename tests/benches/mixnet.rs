use criterion::BenchmarkId;
use criterion::Criterion;
use criterion::{criterion_group, criterion_main};
use futures::StreamExt;
use mixnet_client::MixnetClientMode;
use mixnet_client::{MixnetClient, MixnetClientConfig};
use rand::rngs::OsRng;
use rand::RngCore;

async fn test_one_message(msg_size: usize) {
    let (topology, mut destination_stream) = tests::run_nodes_and_destination_client().await;

    let mut msg = vec![0u8; msg_size];
    rand::thread_rng().fill_bytes(&mut msg);

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
        c.bench_with_input(
            BenchmarkId::new(format!("one message {size}"), size),
            size,
            |b, &s| {
                b.to_async(tokio::runtime::Runtime::new().unwrap())
                    .iter(|| async { test_one_message(s).await });
            },
        );
    }
}

criterion_group!(benches, bench_one_message);
criterion_main!(benches);
