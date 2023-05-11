use serde::Serialize;

use crate::settings::SimulationSettings;
use crate::warding::SimulationState;

pub trait Record: From<SimulationSettings> + Send + Sync + 'static {
    fn is_settings(&self) -> bool;
}

pub type SerializedNodeState = serde_json::Value;

#[derive(Serialize)]
pub enum OutData {
    Settings(Box<SimulationSettings>),
    Data(SerializedNodeState),
}

impl From<SimulationSettings> for OutData {
    fn from(settings: SimulationSettings) -> Self {
        Self::Settings(Box::new(settings))
    }
}

impl From<SerializedNodeState> for OutData {
    fn from(state: SerializedNodeState) -> Self {
        Self::Data(state)
    }
}

impl Record for OutData {
    fn is_settings(&self) -> bool {
        matches!(self, Self::Settings(_))
    }
}

impl OutData {
    #[inline]
    pub const fn new(state: SerializedNodeState) -> Self {
        Self::Data(state)
    }
}

impl<N> TryFrom<&SimulationState<N>> for OutData
where
    N: crate::node::Node,
    N::State: Serialize,
{
    type Error = anyhow::Error;

    fn try_from(state: &crate::warding::SimulationState<N>) -> Result<Self, Self::Error> {
        serde_json::to_value(
            state
                .nodes
                .read()
                .expect("simulations: SimulationState panic when requiring a read lock")
                .iter()
                .map(N::state)
                .collect::<Vec<_>>(),
        )
        .map(OutData::new)
        .map_err(From::from)
    }
}

pub trait NodeStateRecord {
    fn get_serialized_state_record(&self) -> SerializedNodeState {
        SerializedNodeState::Null
    }
}
