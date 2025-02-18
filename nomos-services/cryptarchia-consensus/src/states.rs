// std
use std::marker::PhantomData;
// Crates
use overwatch_rs::services::state::ServiceState;
use serde::{Deserialize, Serialize};
// Internal
use crate::{Cryptarchia, CryptarchiaSettings, Error};
use nomos_core::header::HeaderId;
use nomos_ledger::LedgerState;

pub struct GenesisRecoveryStrategy {
    pub tip: HeaderId,
}

pub struct SecurityRecoveryStrategy {
    pub tip: HeaderId,
    pub security_block_id: HeaderId,
    pub security_ledger_state: LedgerState,
}

pub enum CryptarchiaInitialisationStrategy {
    Genesis,
    RecoveryFromGenesis(GenesisRecoveryStrategy),
    RecoveryFromSecurity(Box<SecurityRecoveryStrategy>),
}

#[derive(Clone, Serialize, Deserialize)]
pub struct CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings> {
    tip: Option<HeaderId>,
    security_block: Option<HeaderId>,
    security_ledger_state: Option<LedgerState>,
    _txs: PhantomData<TxS>,
    _bxs: PhantomData<BxS>,
    _network_adapter_settings: PhantomData<NetworkAdapterSettings>,
    _blend_adapter_settings: PhantomData<BlendAdapterSettings>,
}

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
    CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
{
    pub fn new(
        tip: Option<HeaderId>,
        security_block: Option<HeaderId>,
        security_ledger_state: Option<LedgerState>,
    ) -> Self {
        Self {
            tip,
            security_block,
            security_ledger_state,
            _txs: Default::default(),
            _bxs: Default::default(),
            _network_adapter_settings: Default::default(),
            _blend_adapter_settings: Default::default(),
        }
    }

    pub(crate) fn from_cryptarchia(cryptarchia: &Cryptarchia) -> Self {
        let security_block_header = cryptarchia.consensus.get_security_block_header_id();
        let security_ledger_state = security_block_header
            .and_then(|header| cryptarchia.ledger.state(&header))
            .cloned();

        Self::new(
            Some(cryptarchia.tip()),
            security_block_header,
            security_ledger_state,
        )
    }

    fn can_recover(&self) -> bool {
        // This only checks whether tip is defined, as that's a state variable that should
        // always exist. Other attributes might not be present.
        self.tip.is_some()
    }

    fn can_recover_from_security(&self) -> bool {
        self.can_recover() && self.security_block.is_some() && self.security_ledger_state.is_some()
    }

    pub fn recovery_strategy(&self) -> CryptarchiaInitialisationStrategy {
        if self.can_recover_from_security() {
            let strategy = SecurityRecoveryStrategy {
                tip: self.tip.expect("tip not available"),
                security_block_id: self.security_block.expect("security block not available"),
                security_ledger_state: self
                    .security_ledger_state
                    .clone()
                    .expect("security ledger state not available"),
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

impl<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings> ServiceState
    for CryptarchiaConsensusState<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>
{
    type Settings = CryptarchiaSettings<TxS, BxS, NetworkAdapterSettings, BlendAdapterSettings>;
    type Error = Error;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self::new(None, None, None))
    }
}
