use common_http_client::CommonHttpClient;
use kzgrs_backend::common::share::{DaLightShare, DaShare};
use nomos_core::da::blob::Share;
use reqwest::Url;
use tests::{
    common::da::{disseminate_with_metadata, wait_for_indexed_blob, APP_ID},
    topology::{Topology, TopologyConfig},
};

#[tokio::test]
async fn test_get_share_data() {
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

    wait_for_indexed_blob(executor, app_id, from, to, num_subnets).await;

    let executor_blobs = executor.get_indexer_range(app_id, from..to).await;

    let share = executor_blobs
        .iter()
        .flat_map(|(_, shares)| shares)
        .next()
        .unwrap();

    let exec_url =
        Url::parse(format!("http://{}", executor.config().http.backend_settings.address).as_str())
            .unwrap();

    let client = CommonHttpClient::new(None);
    let commitments = client
        .get_commitments::<DaShare>(exec_url.clone(), share.blob_id().try_into().unwrap())
        .await
        .unwrap();

    assert!(commitments.is_some());

    let share_data = client
        .get_share::<DaShare, DaLightShare>(
            exec_url,
            share.blob_id().try_into().unwrap(),
            share.share_idx(),
        )
        .await
        .unwrap();

    assert!(share_data.is_some());
}
