use serde::{Deserialize, Serialize};
use std::marker::PhantomData;
use thiserror::Error;

use overwatch_rs::services::state::ServiceState;

use crate::TxMempoolSettings;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TxMempoolState<PoolState, PoolSettings, NetworkSettings> {
    pub(crate) pool: Option<PoolState>,
    #[serde(skip)]
    _phantom: PhantomData<(PoolSettings, NetworkSettings)>,
}

impl<PoolState, PoolSettings, NetworkSettings>
    TxMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    pub const fn pool(&self) -> Option<&PoolState> {
        self.pool.as_ref()
    }
}

impl<PoolState, PoolSettings, NetworkSettings> From<PoolState>
    for TxMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    fn from(value: PoolState) -> Self {
        Self {
            pool: Some(value),
            _phantom: PhantomData,
        }
    }
}

#[derive(Error, Debug)]
pub enum Error {}

impl<PoolState, PoolSettings, NetworkSettings> ServiceState
    for TxMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    type Error = Error;
    type Settings = TxMempoolSettings<PoolSettings, NetworkSettings>;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self {
            pool: None,
            _phantom: PhantomData,
        })
    }
}
