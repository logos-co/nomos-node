pub mod config;
pub mod repo;

// std
use std::{fs, path::PathBuf, time::Duration};
// crates
use serde::{Deserialize, Serialize};
// internal
use nomos_tracing_service::TracingSettings;
use tests::topology::configs::{consensus::ConsensusParams, da::DaParams};

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CfgSyncConfig {
    pub port: u16,
    pub n_hosts: usize,
    pub timeout: u64,

    // ConsensusConfig related parameters
    pub security_param: u32,
    pub active_slot_coeff: f64,

    // DaConfig related parameters
    pub subnetwork_size: usize,
    pub dispersal_factor: usize,
    pub num_samples: u16,
    pub num_subnets: u16,
    pub old_blobs_check_interval_secs: u64,
    pub blobs_validity_duration_secs: u64,
    pub global_params_path: String,

    // Tracing params
    pub tracing_settings: TracingSettings,
}

impl CfgSyncConfig {
    pub fn load_from_file(file_path: &PathBuf) -> Result<Self, String> {
        let config_content = fs::read_to_string(file_path)
            .map_err(|err| format!("Failed to read config file: {}", err))?;
        serde_yaml::from_str(&config_content)
            .map_err(|err| format!("Failed to parse config file: {}", err))
    }

    pub fn to_consensus_params(&self) -> ConsensusParams {
        ConsensusParams {
            n_participants: self.n_hosts,
            security_param: self.security_param,
            active_slot_coeff: self.active_slot_coeff,
        }
    }

    pub fn to_da_params(&self) -> DaParams {
        DaParams {
            subnetwork_size: self.subnetwork_size,
            dispersal_factor: self.dispersal_factor,
            num_samples: self.num_samples,
            num_subnets: self.num_subnets,
            old_blobs_check_interval: Duration::from_secs(self.old_blobs_check_interval_secs),
            blobs_validity_duration: Duration::from_secs(self.blobs_validity_duration_secs),
            global_params_path: self.global_params_path.clone(),
        }
    }

    pub fn to_tracing_settings(&self) -> TracingSettings {
        self.tracing_settings.clone()
    }
}
