use std::marker::PhantomData;

use cl::NoteWitness;
use nomos_core::header::HeaderId;
use nomos_ledger::LedgerState;
use overwatch::services::state::ServiceState;
use serde::{Deserialize, Serialize};

use crate::{leadership::Leader, Cryptarchia, CryptarchiaSettings, Error};

/// Indicates that there's stored data so [`Cryptarchia`] should be recovered.
/// However, the number of stored epochs is fewer than
/// [`Config::security_param`](cryptarchia_engine::config::Config).
///
/// As a result, a [`Cryptarchia`](cryptarchia_engine::Cryptarchia) instance
/// must first be built from genesis and then recovered up to the `tip` epoch.
pub struct GenesisRecoveryStrategy {
    pub tip: HeaderId,
}

/// Indicates that there's stored data so [`Cryptarchia`] should be recovered,
/// and the number of stored epochs is larger than
/// [`Config::security_param`](cryptarchia_engine::config::Config).
///
/// As a result, a [`Cryptarchia`](cryptarchia_engine::Cryptarchia) instance
/// must first be built from the security state and then recovered up to the
/// `tip` epoch.
pub struct SecurityRecoveryStrategy {
    pub tip: HeaderId,
    pub security_block_id: HeaderId,
    pub security_ledger_state: LedgerState,
    pub security_leader_notes: Vec<NoteWitness>,
}

pub enum CryptarchiaInitialisationStrategy {
    /// Indicates that there's no stored data so [`Cryptarchia`] should be built
    /// from genesis.
    Genesis,
    RecoveryFromGenesis(GenesisRecoveryStrategy),
    RecoveryFromSecurity(Box<SecurityRecoveryStrategy>),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CryptarchiaConsensusState<
    TxS,
    BxS,
    NetworkAdapterSettings,
    BlendAdapterSettings,
    TimeBackendSettings,
> {
    tip: Option<HeaderId>,
    security_block: Option<HeaderId>,
    security_ledger_state: Option<LedgerState>,
    security_leader_notes: Option<Vec<NoteWitness>>,
    _txs: PhantomData<TxS>,
    _bxs: PhantomData<BxS>,
    _network_adapter_settings: PhantomData<NetworkAdapterSettings>,
    _blend_adapter_settings: PhantomData<BlendAdapterSettings>,
    _time_backend_settings: PhantomData<TimeBackendSettings>,
}

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings, TimeBackendSettings>
    CryptarchiaConsensusState<
        TxS,
        BxS,
        NetworkAdapterSettings,
        BlendAdapterSettings,
        TimeBackendSettings,
    >
{
    pub const fn new(
        tip: Option<HeaderId>,
        security_block: Option<HeaderId>,
        security_ledger_state: Option<LedgerState>,
        security_leader_notes: Option<Vec<NoteWitness>>,
    ) -> Self {
        Self {
            tip,
            security_block,
            security_ledger_state,
            security_leader_notes,
            _txs: PhantomData,
            _bxs: PhantomData,
            _network_adapter_settings: PhantomData,
            _blend_adapter_settings: PhantomData,
            _time_backend_settings: PhantomData,
        }
    }

    pub(crate) fn from_cryptarchia(cryptarchia: &Cryptarchia, leader: &Leader) -> Self {
        let security_block_header = cryptarchia.consensus.get_security_block_header_id();
        let security_ledger_state = security_block_header
            .and_then(|header| cryptarchia.ledger.state(&header))
            .cloned();
        let security_leader_notes = security_block_header
            .and_then(|header_id| leader.notes(&header_id))
            .cloned();

        Self::new(
            Some(cryptarchia.tip()),
            security_block_header,
            security_ledger_state,
            security_leader_notes,
        )
    }

    const fn can_recover(&self) -> bool {
        // This only checks whether tip is defined, as that's a state variable that
        // should always exist. Other attributes might not be present.
        self.tip.is_some()
    }

    const fn can_recover_from_security(&self) -> bool {
        // TODO: Check if one or more (but not all) the security attrs are missing.
        // That's a bug.
        self.can_recover()
            && self.security_block.is_some()
            && self.security_ledger_state.is_some()
            && self.security_leader_notes.is_some()
    }

    pub fn recovery_strategy(&mut self) -> CryptarchiaInitialisationStrategy {
        if self.can_recover_from_security() {
            let strategy = SecurityRecoveryStrategy {
                tip: self.tip.take().expect("tip not available"),
                security_block_id: self
                    .security_block
                    .take()
                    .expect("security block not available"),
                security_ledger_state: self
                    .security_ledger_state
                    .take()
                    .expect("security ledger state not available"),
                security_leader_notes: self
                    .security_leader_notes
                    .take()
                    .expect("security leader notes not available"),
            };
            CryptarchiaInitialisationStrategy::RecoveryFromSecurity(Box::new(strategy))
        } else if self.can_recover() {
            let strategy = GenesisRecoveryStrategy {
                tip: self.tip.expect("tip not available"),
            };
            CryptarchiaInitialisationStrategy::RecoveryFromGenesis(strategy)
        } else {
            CryptarchiaInitialisationStrategy::Genesis
        }
    }
}

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings, TimeBackendSettings> ServiceState
    for CryptarchiaConsensusState<
        TxS,
        BxS,
        NetworkAdapterSettings,
        BlendAdapterSettings,
        TimeBackendSettings,
    >
{
    type Settings = CryptarchiaSettings<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>;
    type Error = Error;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self::new(None, None, None, None))
    }
}
