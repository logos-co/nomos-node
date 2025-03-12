use std::{convert::Infallible, marker::PhantomData};

use overwatch::services::state::ServiceState;
use serde::{Deserialize, Serialize};

use crate::da::settings::DaMempoolSettings;

/// State that is maintained across service restarts.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct DaMempoolState<PoolState, PoolSettings, NetworkSettings> {
    /// The (optional) pool snapshot.
    pub(crate) pool: Option<PoolState>,
    #[serde(skip)]
    _phantom: PhantomData<(PoolSettings, NetworkSettings)>,
}

impl<PoolState, PoolSettings, NetworkSettings>
    DaMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    pub const fn pool(&self) -> Option<&PoolState> {
        self.pool.as_ref()
    }
}

impl<PoolState, PoolSettings, NetworkSettings> From<PoolState>
    for DaMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    fn from(value: PoolState) -> Self {
        Self {
            pool: Some(value),
            _phantom: PhantomData,
        }
    }
}

impl<PoolState, PoolSettings, NetworkSettings> ServiceState
    for DaMempoolState<PoolState, PoolSettings, NetworkSettings>
{
    type Error = Infallible;
    type Settings = DaMempoolSettings<PoolSettings, NetworkSettings>;

    fn from_settings(_settings: &Self::Settings) -> Result<Self, Self::Error> {
        Ok(Self {
            pool: None,
            _phantom: PhantomData,
        })
    }
}
