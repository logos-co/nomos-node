use std::net::{Ipv4Addr, SocketAddr};

use kzgrs_backend::{
    common::{
        blob::{DaBlob, DaBlobSharedCommitments, DaLightBlob},
        Column,
    },
    encoder::DaEncoderParams,
};
use nomos_api::ApiServiceSettings;
use nomos_core::da::{blob::Blob, DaEncoder};
use nomos_da_storage::rocksdb::{
    create_blob_idx, key_bytes, DA_BLOB_PREFIX, DA_SHARED_COMMITMENTS_PREFIX,
};
use nomos_libp2p::libp2p::bytes::Bytes;
use nomos_node::{
    api::{backend::AxumBackendSettings, handlers::GetLightBlobReq},
    NomosApiService, Wire,
};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use nomos_tracing_service::{Tracing, TracingSettings};
use overwatch_rs::{
    overwatch::OverwatchRunner, services::relay::OutboundRelay, OpaqueServiceHandle, Services,
};
use rand::RngCore;
use reqwest::Url;
use serde::{de::DeserializeOwned, Serialize};
use tests::{get_available_port, GLOBAL_PARAMS_PATH};

// Probably temporary solution until can be integrated into integration tests
#[derive(Services)]
pub struct TestApiStorageCallsNode {
    tracing: OpaqueServiceHandle<Tracing>,
    http: OpaqueServiceHandle<NomosApiService>,
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
}

#[test]
fn test_get_blob_data() {
    let global_parameters =
        kzgrs_backend::global::global_parameters_from_file(GLOBAL_PARAMS_PATH.as_str()).unwrap();
    let encoder_params = DaEncoderParams::new(2, false, global_parameters);
    let encoder = kzgrs_backend::encoder::DaEncoder::new(encoder_params);
    let data = rand_data(10);
    let encoded_data = encoder.encode(&data).unwrap();
    let da_blob = DaBlob {
        column: Column(Default::default()),
        column_idx: 0,
        column_commitment: encoded_data.column_commitments[0],
        aggregated_column_commitment: encoded_data.aggregated_column_commitment,
        aggregated_column_proof: encoded_data.aggregated_column_proofs[0],
        rows_commitments: encoded_data.row_commitments,
        rows_proofs: encoded_data.rows_proofs[0].clone(),
    };

    let http_addr = SocketAddr::new(Ipv4Addr::LOCALHOST.into(), get_available_port());
    let app = OverwatchRunner::<TestApiStorageCallsNode>::run(
        TestApiStorageCallsNodeServiceSettings {
            tracing: TracingSettings::default(),
            http: ApiServiceSettings {
                backend_settings: AxumBackendSettings {
                    address: http_addr,
                    cors_origins: vec![],
                },
            },
            storage: nomos_storage::backends::rocksdb::RocksBackendSettings {
                db_path: "./db".into(),
                read_only: false,
                column_family: None,
            },
        },
        None,
    )
    .unwrap();

    let (finished_tx, finished_rx) = tokio::sync::oneshot::channel();
    let storage_relay = app.handle().relay::<StorageService<RocksBackend<Wire>>>();
    app.spawn(async move {
        let storage_outbound = storage_relay.connect().await.unwrap();
        let (light_blob, shared_commitments) = da_blob.into_blob_and_shared_commitments();

        let blob_id = &[0; 32];
        let column_idx = &[0; 2];
        store(
            &storage_outbound,
            key_bytes(DA_SHARED_COMMITMENTS_PREFIX, blob_id),
            &shared_commitments,
        )
        .await;

        let blob_idx = create_blob_idx(blob_id.as_ref(), column_idx);
        store(
            &storage_outbound,
            key_bytes(DA_BLOB_PREFIX, blob_idx),
            &light_blob,
        )
        .await;

        let url = format!("http://{}/da/get-shared-commitments", http_addr);
        let received_shared_commitments: DaBlobSharedCommitments =
            fetch_from_api(&url, &blob_id[..]).await;

        let url = format!("http://{}/da/get-blob", http_addr);
        let req: GetLightBlobReq<DaBlob> = GetLightBlobReq {
            blob_id: blob_id.clone(),
            column_idx: column_idx.clone(),
        };
        let received_blob: DaLightBlob = fetch_from_api(&url, &req).await;
        finished_tx
            .send(received_blob == light_blob && received_shared_commitments == shared_commitments)
            .unwrap();
    });

    let equal = app.runtime().block_on(finished_rx).unwrap();
    assert!(equal);
}

fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}

async fn store<V: Serialize, K: AsRef<[u8]>>(
    relay: &OutboundRelay<StorageMsg<RocksBackend<Wire>>>,
    key: K,
    value: &V,
) {
    relay
        .send(StorageMsg::Store {
            key: Bytes::from(key.as_ref().to_vec()),
            value: Wire::serialize(value),
        })
        .await
        .unwrap();
}

async fn fetch_from_api<T: DeserializeOwned, R: Serialize + ?Sized>(url: &str, req: &R) -> T {
    let url = Url::parse(url).unwrap();
    let client = reqwest::Client::new();
    let request = client.get(url).json(req).build().unwrap();
    let response = client.execute(request).await.unwrap();

    assert_eq!(response.status(), 200);
    let body = response.text().await.unwrap();
    serde_json::from_str(&body).unwrap()
}
