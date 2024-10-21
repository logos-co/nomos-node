pub mod consensus;
pub mod da;
pub mod mix;
pub mod network;

use consensus::GeneralConsensusConfig;
use da::GeneralDaConfig;
use mix::GeneralMixConfig;
use network::GeneralNetworkConfig;

#[derive(Clone)]
pub struct GeneralConfig {
    pub consensus_config: GeneralConsensusConfig,
    pub da_config: GeneralDaConfig,
    pub network_config: GeneralNetworkConfig,
    pub mix_config: GeneralMixConfig,
}
