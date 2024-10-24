pub mod api;
pub mod consensus;
pub mod da;
pub mod mix;
pub mod network;
pub mod tracing;

use api::GeneralApiConfig;
use consensus::GeneralConsensusConfig;
use da::GeneralDaConfig;
use mix::GeneralMixConfig;
use network::GeneralNetworkConfig;
use tracing::GeneralTracingConfig;

#[derive(Clone)]
pub struct GeneralConfig {
    pub api_config: GeneralApiConfig,
    pub consensus_config: GeneralConsensusConfig,
    pub da_config: GeneralDaConfig,
    pub network_config: GeneralNetworkConfig,
    pub mix_config: GeneralMixConfig,
    pub tracing_config: GeneralTracingConfig,
}
