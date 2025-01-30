mod binaries;

use std::time::Duration;

use cfgsync::CfgSyncConfig;
use nomos_executor::config::Config as ExecutorConfig;
use nomos_node::Config as ValidatorConfig;
use nomos_node::Tracing;
use nomos_tracing_service::TracingSettings;
use reqwest::Client;
use serde::de::DeserializeOwned;
use serde::Serialize;
use std::net::Ipv4Addr;
use tests::get_available_port;
use tokio::join;
use tracing::Level;

#[derive(Serialize)]
struct ClientIp {
    ip: Ipv4Addr,
    identifier: String,
}

async fn get_config<Config: Serialize + DeserializeOwned>(
    ip: Ipv4Addr,
    identifier: String,
    url: &str,
    config_file: &str,
) -> Result<(), String> {
    let client = Client::new();

    let response = client
        .post(url)
        .json(&ClientIp { ip, identifier })
        .send()
        .await
        .map_err(|err| format!("Failed to send IP announcement: {}", err))?;

    if !response.status().is_success() {
        return Err(format!("Server error: {:?}", response.status()));
    }
    let body = response
        .text()
        .await
        .map_err(|err| format!("Failed to read response body: {}", err))?;
    println!(">>>>>> Response Body: {}", body);

    Ok(())
}

#[tokio::test]
async fn two_nodes() {
    let config = CfgSyncConfig {
        port: get_available_port(),
        n_hosts: 2,
        timeout: 10,

        // ConsensusConfig related parameters
        security_param: 10,
        active_slot_coeff: 0.9,

        // DaConfig related parameters
        subnetwork_size: 2,
        dispersal_factor: 2,
        num_samples: 1,
        num_subnets: 2,
        old_blobs_check_interval_secs: 5,
        blobs_validity_duration_secs: 60,
        global_params_path: "/kzgrs_test_params".to_string(),

        // Tracing settings
        tracing_settings: TracingSettings {
            logger: nomos_tracing_service::LoggerLayer::Stdout,
            tracing: nomos_tracing_service::TracingLayer::None,
            filter: nomos_tracing_service::FilterLayer::None,
            metrics: nomos_tracing_service::MetricsLayer::None,
            level: Level::DEBUG,
        },
    };

    let server = binaries::server::CfgSync::spawn(config.clone()).await;

    tokio::time::sleep(Duration::from_secs(2)).await;

    let server_addr = format!("http://127.0.0.1:{}", config.port);
    let validator_endpoint = format!("{}/validator", server_addr);
    let executor_endpoint = format!("{}/executor", server_addr);

    let ip1 = Ipv4Addr::LOCALHOST;
    let identifier1 = "node-1".to_string();
    let config_file_path1 = "test_config_1.yaml".to_string();

    let ip2 = Ipv4Addr::LOCALHOST;
    let identifier2 = "node-2".to_string();
    let config_file_path2 = "test_config_2.yaml".to_string();

    let (result1, result2) = join!(
        get_config::<ValidatorConfig>(ip1, identifier1, &validator_endpoint, &config_file_path1),
        get_config::<ExecutorConfig>(ip2, identifier2, &executor_endpoint, &config_file_path2)
    );
}
