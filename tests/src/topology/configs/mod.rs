pub mod api;
pub mod blend;
pub mod consensus;
pub mod da;
pub mod network;
pub mod tracing;

use api::GeneralApiConfig;
use blend::GeneralBlendConfig;
use consensus::GeneralConsensusConfig;
use da::GeneralDaConfig;
use network::GeneralNetworkConfig;
use tracing::GeneralTracingConfig;

#[derive(Clone)]
pub struct GeneralConfig {
    pub api_config: GeneralApiConfig,
    pub consensus_config: GeneralConsensusConfig,
    pub da_config: GeneralDaConfig,
    pub network_config: GeneralNetworkConfig,
    pub blend_config: GeneralBlendConfig,
    pub tracing_config: GeneralTracingConfig,
}
