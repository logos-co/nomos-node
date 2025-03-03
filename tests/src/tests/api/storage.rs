use std::net::{Ipv4Addr, SocketAddr};

use kzgrs_backend::{common::blob::DaBlobSharedCommitments, encoder::DaEncoderParams};
use nomos_api::ApiServiceSettings;
use nomos_core::da::DaEncoder;
use nomos_da_storage::rocksdb::{key_bytes, DA_SHARED_COMMITMENTS_PREFIX};
use nomos_node::{api::backend::AxumBackendSettings, NomosApiService, Wire};
use nomos_storage::{
    backends::{rocksdb::RocksBackend, StorageSerde},
    StorageMsg, StorageService,
};
use nomos_tracing_service::{Tracing, TracingSettings};
use overwatch_rs::{overwatch::OverwatchRunner, OpaqueServiceHandle, Services};
use rand::RngCore;
use reqwest::Url;
use tests::{get_available_port, GLOBAL_PARAMS_PATH};

// Probably temporary solution until can be integrated into integration tests
#[derive(Services)]
pub struct TestApiStorageCallsNode {
    tracing: OpaqueServiceHandle<Tracing>,
    http: OpaqueServiceHandle<NomosApiService>,
    storage: OpaqueServiceHandle<StorageService<RocksBackend<Wire>>>,
}

#[test]
fn test_get_shared_commitments() {
    let global_parameters =
        kzgrs_backend::global::global_parameters_from_file(GLOBAL_PARAMS_PATH.as_str()).unwrap();
    let encoder_params = DaEncoderParams::new(2, false, global_parameters);
    let encoder = kzgrs_backend::encoder::DaEncoder::new(encoder_params);
    let data = rand_data(10);
    let encoded_data = encoder.encode(&data).unwrap();

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
        let shared_commitments = DaBlobSharedCommitments {
            aggregated_column_commitment: encoded_data.aggregated_column_commitment,
            rows_commitments: encoded_data.row_commitments,
        };

        let commitments_id = &[0; 32];
        let storage_key = key_bytes(DA_SHARED_COMMITMENTS_PREFIX, commitments_id);
        storage_outbound
            .send(StorageMsg::Store {
                key: storage_key,
                value: Wire::serialize(&shared_commitments),
            })
            .await
            .unwrap();

        let client = reqwest::Client::new();
        let url = Url::parse(&format!("http://{http_addr}/da/get-shared-commitments")).unwrap();
        let request = client.get(url).json(&commitments_id).build().unwrap();
        let response = client.execute(request).await.unwrap();

        assert_eq!(response.status(), 200);
        let body = response.text().await.unwrap();
        let received_shared_commitments: DaBlobSharedCommitments =
            serde_json::from_str(&body).unwrap();
        finished_tx
            .send(received_shared_commitments == shared_commitments)
            .unwrap();
    });

    let equal = tokio::runtime::Runtime::new()
        .unwrap()
        .block_on(finished_rx)
        .unwrap();
    assert!(equal);
}

fn rand_data(elements_count: usize) -> Vec<u8> {
    let mut buff = vec![0; elements_count * DaEncoderParams::MAX_BLS12_381_ENCODING_CHUNK_SIZE];
    rand::thread_rng().fill_bytes(&mut buff);
    buff
}
