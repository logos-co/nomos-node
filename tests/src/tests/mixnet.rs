use futures::StreamExt;
use mixnet_client::{MixnetClient, MixnetClientConfig, MixnetClientMode};
use rand::{rngs::OsRng, RngCore};
use tests::get_available_port;
use tokio::time::Instant;

#[tokio::test]
async fn mixnet_one_small_message() {
    // Similar size as Nomos messages.
    // But, this msg won't be splitted into multiple packets,
    // since min packet size is 2KB (hardcoded in nymsphinx).
    // https://github.com/nymtech/nym/blob/3748ab77a132143d5fd1cd75dd06334d33294815/common/nymsphinx/params/src/packet_sizes.rs#L28C10-L28C10
    test_one_message(500).await
}

#[tokio::test]
async fn mixnet_one_message() {
    test_one_message(100 * 1024).await
}

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

    let start_time = Instant::now();

    let res = sender_client.send(msg.to_vec());
    assert!(res.is_ok());

    let received = destination_stream.next().await.unwrap().unwrap();
    assert_eq!(msg, received.as_slice());

    let elapsed = start_time.elapsed();
    println!("ELAPSED: {elapsed:?}");
}

// TODO: This test should be enabled after the connection reuse is implemented:
//       https://github.com/logos-co/nomos-node/issues/313
//       Currently, this test fails with `Too many open files (os error 24)`.
#[ignore]
#[tokio::test(flavor = "multi_thread")]
async fn mixnet_ten_messages() {
    let (topology, mut destination_stream) = tests::run_nodes_and_destination_client().await;

    let mut msg = [0u8; 100 * 1024];
    rand::thread_rng().fill_bytes(&mut msg);

    let start_time = Instant::now();

    const NUM_MESSAGES: usize = 10;

    for _ in 0..NUM_MESSAGES {
        let msg = msg.clone();
        let topology = topology.clone();

        tokio::spawn(async move {
            let mut sender_client = MixnetClient::new(
                MixnetClientConfig {
                    mode: MixnetClientMode::Sender,
                    topology,
                },
                OsRng,
            );
            let res = sender_client.send(msg.to_vec());
            assert!(res.is_ok());
        });
    }

    for _ in 0..NUM_MESSAGES {
        let received = destination_stream.next().await.unwrap().unwrap();
        assert_eq!(msg, received.as_slice());
    }

    let elapsed = start_time.elapsed();
    println!("ELAPSED: {elapsed:?}");
}
