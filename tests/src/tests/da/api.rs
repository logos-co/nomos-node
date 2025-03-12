use common_http_client::CommonHttpClient;
use kzgrs_backend::common::blob::{DaBlob, DaLightBlob};
use nomos_core::da::blob::Blob;
use nomos_libp2p::ed25519;
use rand::{rngs::OsRng, RngCore};
use reqwest::Url;
use subnetworks_assignations::MembershipHandler;
use tests::{
    common::da::{disseminate_with_metadata, wait_for_indexed_blob, APP_ID},
    secret_key_to_peer_id,
    topology::{Topology, TopologyConfig},
};

#[tokio::test]
async fn test_get_blob_data() {
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

    let blob = executor_blobs
        .iter()
        .flat_map(|(_, blobs)| blobs)
        .next()
        .unwrap();

    let exec_url =
        Url::parse(format!("http://{}", executor.config().http.backend_settings.address).as_str())
            .unwrap();

    let client = CommonHttpClient::new(None);
    let commitments = client
        .get_commitments::<DaBlob>(exec_url.clone(), blob.id().try_into().unwrap())
        .await
        .unwrap();

    assert!(commitments.is_some());

    let blob_data = client
        .get_blob::<DaBlob, DaLightBlob>(exec_url, blob.id().try_into().unwrap(), blob.column_idx())
        .await
        .unwrap();

    assert!(blob_data.is_some());
}

#[tokio::test]
async fn test_block_peer() {
    let topology = Topology::spawn(TopologyConfig::validator_and_executor()).await;
    let executor = &topology.executors()[0];

    let blacklisted_peers = executor.blacklisted_peers().await;
    assert!(blacklisted_peers.is_empty());

    let membership = executor
        .config()
        .da_network
        .backend
        .validator_settings
        .membership
        .members();

    let existing_peer_id = membership
        .iter()
        .next()
        .expect("Expected at least one member in the set");

    // try block/unblock peer id combinations
    let blocked = executor.block_peer(existing_peer_id.to_string()).await;
    assert!(blocked);

    let blacklisted_peers = executor.blacklisted_peers().await;
    assert_eq!(blacklisted_peers.len(), 1);
    assert_eq!(blacklisted_peers[0], existing_peer_id.to_string());

    let blocked = executor.block_peer(existing_peer_id.to_string()).await;
    assert!(!blocked);

    let unblocked = executor.unblock_peer(existing_peer_id.to_string()).await;
    assert!(unblocked);

    let blacklisted_peers = executor.blacklisted_peers().await;
    assert!(blacklisted_peers.is_empty());

    let unblocked = executor.unblock_peer(existing_peer_id.to_string()).await;
    assert!(!unblocked);

    // try blocking/unblocking non existing peer id
    let mut node_key_bytes = [0u8; 32];
    OsRng.fill_bytes(&mut node_key_bytes);

    let node_key = ed25519::SecretKey::try_from_bytes(&mut node_key_bytes)
        .expect("Failed to generate secret key from bytes");

    let non_existing_peer_id = secret_key_to_peer_id(node_key);
    let blocked = executor.block_peer(non_existing_peer_id.to_string()).await;
    assert!(!blocked);

    let unblocked = executor
        .unblock_peer(non_existing_peer_id.to_string())
        .await;
    assert!(!unblocked);
}
