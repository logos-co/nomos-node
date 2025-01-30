mod binaries;

use cfgsync::CfgSyncConfig;
use nomos_node::Tracing;
use nomos_tracing_service::TracingSettings;
use tests::get_available_port;
use tracing::Level;

#[test]
fn two_nodes() {
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
    let server = binaries::server::CfgSync::spawn(config);
}
