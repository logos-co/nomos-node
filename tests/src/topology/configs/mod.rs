pub mod consensus;
pub mod da;
pub mod network;

use consensus::GeneralConsensusConfig;
use da::GeneralDaConfig;
use network::GeneralNetworkConfig;

#[derive(Clone)]
pub struct GeneralConfig {
    pub consensus_config: GeneralConsensusConfig,
    pub da_config: GeneralDaConfig,
    pub network_config: GeneralNetworkConfig,
}
