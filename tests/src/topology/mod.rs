pub mod configs;

use configs::{
    da::{create_da_configs, DaParams},
    network::{create_network_configs, NetworkParams},
    tracing::create_tracing_configs,
    GeneralConfig,
};
use rand::{thread_rng, Rng};

use crate::{
    nodes::{
        executor::{create_executor_config, Executor},
        validator::{create_validator_config, Validator},
    },
    topology::configs::{
        api::create_api_configs,
        consensus::{create_consensus_configs, ConsensusParams},
        mix::create_mix_configs,
    },
};

pub struct TopologyConfig {
    n_validators: usize,
    n_executors: usize,
    consensus_params: ConsensusParams,
    da_params: DaParams,
    network_params: NetworkParams,
}

impl TopologyConfig {
    pub fn two_validators() -> TopologyConfig {
        TopologyConfig {
            n_validators: 2,
            n_executors: 0,
            consensus_params: ConsensusParams::default_for_participants(2),
            da_params: Default::default(),
            network_params: Default::default(),
        }
    }

    pub fn validator_and_executor() -> TopologyConfig {
        TopologyConfig {
            n_validators: 1,
            n_executors: 1,
            consensus_params: ConsensusParams::default_for_participants(2),
            da_params: DaParams {
                dispersal_factor: 2,
                subnetwork_size: 2,
                num_subnets: 2,
                ..Default::default()
            },
            network_params: Default::default(),
        }
    }
}

pub struct Topology {
    validators: Vec<Validator>,
    executors: Vec<Executor>,
}

impl Topology {
    pub async fn spawn(config: TopologyConfig) -> Self {
        let n_participants = config.n_validators + config.n_executors;

        // we use the same random bytes for:
        // * da id
        // * coin sk
        // * coin nonce
        // * libp2p node key
        let mut ids = vec![[0; 32]; n_participants];
        for id in &mut ids {
            thread_rng().fill(id);
        }

        let consensus_configs = create_consensus_configs(&ids, config.consensus_params);
        let da_configs = create_da_configs(&ids, config.da_params);
        let network_configs = create_network_configs(&ids, config.network_params);
        let mix_configs = create_mix_configs(&ids);
        let api_configs = create_api_configs(&ids);
        let tracing_configs = create_tracing_configs(&ids);

        let mut validators = Vec::new();
        for i in 0..config.n_validators {
            let config = create_validator_config(GeneralConfig {
                consensus_config: consensus_configs[i].to_owned(),
                da_config: da_configs[i].to_owned(),
                network_config: network_configs[i].to_owned(),
                mix_config: mix_configs[i].to_owned(),
                api_config: api_configs[i].to_owned(),
                tracing_config: tracing_configs[i].to_owned(),
            });
            validators.push(Validator::spawn(config).await)
        }

        let mut executors = Vec::new();
        for i in config.n_validators..n_participants {
            let config = create_executor_config(GeneralConfig {
                consensus_config: consensus_configs[i].to_owned(),
                da_config: da_configs[i].to_owned(),
                network_config: network_configs[i].to_owned(),
                mix_config: mix_configs[i].to_owned(),
                api_config: api_configs[i].to_owned(),
                tracing_config: tracing_configs[i].to_owned(),
            });
            executors.push(Executor::spawn(config).await)
        }

        Self {
            validators,
            executors,
        }
    }

    pub fn validators(&self) -> &[Validator] {
        &self.validators
    }

    pub fn executors(&self) -> &[Executor] {
        &self.executors
    }
}
