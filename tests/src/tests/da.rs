use executor_http_client::ExecutorHttpClient;

use reqwest::ClientBuilder;
use reqwest::Url;
use std::time::Duration;
use tests::nodes::executor::Executor;
use tests::topology::Topology;
use tests::topology::TopologyConfig;

const APP_ID: &str = "fd3384e132ad02a56c78f45547ee40038dc79002b90d29ed90e08eee762ae715";

async fn disseminate(executor: &Executor) {
    // Nomos Cli is acting as the first node when dispersing the data by using the key associated
    // with that Nomos Node.
    let executor_config = executor.config();

    let client = ClientBuilder::new()
        .build()
        .expect("Client from default settings should be able to build");

    let backend_address = executor_config.http.backend_settings.address;
    let exec_url = Url::parse(&format!("http://{}", backend_address)).unwrap();
    let client = ExecutorHttpClient::new(client, exec_url);

    let data = [1u8; 31];

    let app_id = hex::decode(APP_ID).unwrap();
    let metadata = kzgrs_backend::dispersal::Metadata::new(app_id.try_into().unwrap(), 0u64.into());
    client.publish_blob(data.to_vec(), metadata).await.unwrap();
}

#[tokio::test]
async fn disseminate_and_retrieve() {
    let topology = Topology::spawn(TopologyConfig::validator_and_executor()).await;
    let executor = &topology.executors()[0];
    let validator = &topology.validators()[0];

    tokio::time::sleep(Duration::from_secs(15)).await;
    disseminate(executor).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    let from = 0u64.to_be_bytes();
    let to = 1u64.to_be_bytes();
    let app_id = hex::decode(APP_ID).unwrap();

    let executor_blobs = executor
        .get_indexer_range(app_id.clone().try_into().unwrap(), from..to)
        .await;
    let validator_blobs = validator
        .get_indexer_range(app_id.try_into().unwrap(), from..to)
        .await;

    let executor_idx_0_blobs: Vec<_> = executor_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs)
        .collect();
    let validator_idx_0_blobs: Vec<_> = validator_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs)
        .collect();

    // Index zero shouldn't be empty, node 2 replicated both blobs to node 1 because they both
    // are in the same subnetwork.)
    assert!(executor_idx_0_blobs.len() == 2);
    assert!(validator_idx_0_blobs.len() == 2);
    for b in executor_idx_0_blobs.iter() {
        assert!(!b.is_empty())
    }

    for b in validator_idx_0_blobs.iter() {
        assert!(!b.is_empty())
    }
}
