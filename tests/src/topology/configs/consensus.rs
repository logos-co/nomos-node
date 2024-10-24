use std::str::FromStr;
use std::time::Duration;

use cl::{NoteWitness, NullifierSecret};
use cryptarchia_consensus::{LeaderConfig, TimeConfig};
use nomos_core::staking::NMO_UNIT;
use nomos_ledger::LedgerState;
use rand::thread_rng;
use time::OffsetDateTime;

const DEFAULT_SLOT_TIME: u64 = 2;
const CONSENSUS_SLOT_TIME_VAR: &str = "CONSENSUS_SLOT_TIME";

#[derive(Clone)]
pub struct ConsensusParams {
    pub n_participants: usize,
    pub security_param: u32,
    pub active_slot_coeff: f64,
}

impl ConsensusParams {
    pub fn default_for_participants(n_participants: usize) -> Self {
        ConsensusParams {
            n_participants,
            // by setting the slot coeff to 1, we also increase the probability of multiple blocks (forks)
            // being produced in the same slot (epoch). Setting the security parameter to some value > 1
            // ensures nodes have some time to sync before deciding on the longest chain.
            security_param: 10,
            // a block should be produced (on average) every slot
            active_slot_coeff: 0.9,
        }
    }
}

/// General consensus configuration for a chosen participant, that later could be converted into a
/// specific service or services configuration.
#[derive(Clone)]
pub struct GeneralConsensusConfig {
    pub leader_config: LeaderConfig,
    pub ledger_config: nomos_ledger::Config,
    pub genesis_state: LedgerState,
    pub time: TimeConfig,
}

pub fn create_consensus_configs(
    ids: &[[u8; 32]],
    consensus_params: ConsensusParams,
) -> Vec<GeneralConsensusConfig> {
    let notes = (0..ids.len())
        .map(|_| NoteWitness::basic(1, NMO_UNIT, &mut thread_rng()))
        .collect::<Vec<_>>();

    let sks = ids
        .iter()
        .map(|&id| {
            let mut sk = [0; 16];
            sk.copy_from_slice(&id[0..16]);
            NullifierSecret(sk)
        })
        .collect::<Vec<_>>();

    // no commitments for now, proofs are not checked anyway
    let genesis_state = LedgerState::from_commitments(
        notes.iter().zip(&sks).map(|(n, sk)| n.commit(sk.commit())),
        (ids.len() as u32).into(),
    );
    let ledger_config = nomos_ledger::Config {
        epoch_stake_distribution_stabilization: 3,
        epoch_period_nonce_buffer: 3,
        epoch_period_nonce_stabilization: 4,
        consensus_config: cryptarchia_engine::Config {
            security_param: consensus_params.security_param,
            active_slot_coeff: consensus_params.active_slot_coeff,
        },
    };
    let slot_duration = std::env::var(CONSENSUS_SLOT_TIME_VAR)
        .map(|s| <u64>::from_str(&s).unwrap())
        .unwrap_or(DEFAULT_SLOT_TIME);
    let time_config = TimeConfig {
        slot_duration: Duration::from_secs(slot_duration),
        chain_start_time: OffsetDateTime::now_utc(),
    };

    notes
        .into_iter()
        .zip(sks)
        .map(|(note, nf_sk)| GeneralConsensusConfig {
            leader_config: LeaderConfig {
                notes: vec![note],
                nf_sk,
            },
            ledger_config: ledger_config.clone(),
            genesis_state: genesis_state.clone(),
            time: time_config.clone(),
        })
        .collect()
}
