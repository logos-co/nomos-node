use std::time::Duration;

use executor_http_client::ExecutorHttpClient;
use kzgrs_backend::{common::blob::DaBlob, reconstruction::reconstruct_without_missing_data};
use reqwest::{ClientBuilder, Url};
use tests::{
    adjust_timeout,
    nodes::executor::Executor,
    topology::{Topology, TopologyConfig},
};

const APP_ID: &str = "fd3384e132ad02a56c78f45547ee40038dc79002b90d29ed90e08eee762ae715";
const DA_TESTS_TIMEOUT: u64 = 120;

async fn disseminate_with_metadata(
    executor: &Executor,
    data: &[u8],
    metadata: kzgrs_backend::dispersal::Metadata,
) {
    let executor_config = executor.config();

    let client = ClientBuilder::new()
        .build()
        .expect("Client from default settings should be able to build");

    let backend_address = executor_config.http.backend_settings.address;
    let exec_url = Url::parse(&format!("http://{}", backend_address)).unwrap();
    let client = ExecutorHttpClient::new(client, exec_url, None);

    client.publish_blob(data.to_vec(), metadata).await.unwrap();
}

#[ignore = "for manual usage, disseminate_retrieve_reconstruct is preferred for ci"]
#[tokio::test]
async fn disseminate_and_retrieve() {
    let topology = Topology::spawn(TopologyConfig::validator_and_executor()).await;
    let executor = &topology.executors()[0];
    let validator = &topology.validators()[0];

    let data = [1u8; 31];
    let app_id = hex::decode(APP_ID).unwrap();
    let metadata =
        kzgrs_backend::dispersal::Metadata::new(app_id.clone().try_into().unwrap(), 0u64.into());

    tokio::time::sleep(Duration::from_secs(15)).await;
    disseminate_with_metadata(executor, &data, metadata).await;
    tokio::time::sleep(Duration::from_secs(20)).await;

    let from = 0u64.to_be_bytes();
    let to = 1u64.to_be_bytes();

    let executor_blobs = executor
        .get_indexer_range(app_id.clone().try_into().unwrap(), from..to)
        .await;
    let validator_blobs = validator
        .get_indexer_range(app_id.try_into().unwrap(), from..to)
        .await;

    let executor_idx_0_blobs = executor_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs);
    let validator_idx_0_blobs = validator_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs);

    // Index zero shouldn't be empty, validator replicated both blobs to executor
    // because they both are in the same subnetwork.
    assert!(executor_idx_0_blobs.count() == 2);
    assert!(validator_idx_0_blobs.count() == 2);
}

#[tokio::test]
async fn disseminate_retrieve_reconstruct() {
    let topology = Topology::spawn(TopologyConfig::validator_and_executor()).await;
    let executor = &topology.executors()[0];
    let num_subnets = executor.config().da_network.backend.num_subnets as usize;

    let data = [1u8; 31];
    let app_id = hex::decode(APP_ID).unwrap();
    let app_id: [u8; 32] = app_id.clone().try_into().unwrap();
    let metadata = kzgrs_backend::dispersal::Metadata::new(app_id, 0u64.into());

    disseminate_with_metadata(executor, &data, metadata).await;

    let from = 0u64.to_be_bytes();
    let to = 1u64.to_be_bytes();

    let blobs_fut = async {
        let mut num_blobs = 0;
        while num_blobs < num_subnets {
            let executor_blobs = executor.get_indexer_range(app_id, from..to).await;
            num_blobs = executor_blobs
                .into_iter()
                .filter(|(i, _)| i == &from)
                .flat_map(|(_, blobs)| blobs)
                .collect::<Vec<DaBlob>>()
                .len();
        }
    };

    let timeout = adjust_timeout(Duration::from_secs(DA_TESTS_TIMEOUT));
    if (tokio::time::timeout(timeout, blobs_fut).await).is_err() {
        panic!("timed out waiting for indexed blob");
    }

    let executor_blobs = executor.get_indexer_range(app_id, from..to).await;
    let executor_idx_0_blobs: Vec<_> = executor_blobs
        .iter()
        .filter(|(i, _)| i == &from)
        .flat_map(|(_, blobs)| blobs)
        .collect();

    // Reconstruction is performed from the one of the two blobs.
    let blobs = vec![executor_idx_0_blobs[0].clone()];
    let reconstructed = reconstruct_without_missing_data(&blobs);
    assert_eq!(reconstructed, data);
}

#[ignore = "for local debugging"]
#[tokio::test]
async fn local_testnet() {
    let topology = Topology::spawn(TopologyConfig::validators_and_executor(3, 2)).await;
    let executor = &topology.executors()[0];
    let app_id = hex::decode(APP_ID).expect("Invalid APP_ID");

    let mut index = 0u64;
    loop {
        disseminate_with_metadata(
            executor,
            &generate_data(index),
            create_metadata(&app_id, index),
        )
        .await;

        index += 1;
        tokio::time::sleep(Duration::from_secs(30)).await;
    }
}

fn generate_data(index: u64) -> Vec<u8> {
    (index as u8..index as u8 + 31).collect()
}

fn create_metadata(app_id: &[u8], index: u64) -> kzgrs_backend::dispersal::Metadata {
    kzgrs_backend::dispersal::Metadata::new(
        app_id.try_into().expect("Failed to convert APP_ID"),
        index.into(),
    )
}
