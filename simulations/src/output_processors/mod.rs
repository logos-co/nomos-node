use serde::Serialize;

use crate::warding::SimulationState;

pub type SerializedNodeState = serde_json::Value;

#[derive(Serialize)]
pub struct OutData(SerializedNodeState);

impl OutData {
    #[inline]
    pub const fn new(state: SerializedNodeState) -> Self {
        Self(state)
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
