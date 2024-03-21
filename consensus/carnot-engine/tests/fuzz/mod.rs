mod ref_state;
pub mod sut;
mod transition;

type Block = carnot_engine::Block<[u8; 32]>;
type AggregateQc = carnot_engine::AggregateQc<[u8; 32]>;
type Qc = carnot_engine::Qc<[u8; 32]>;
type StandardQc = carnot_engine::StandardQc<[u8; 32]>;
type TimeoutQc = carnot_engine::TimeoutQc<[u8; 32]>;
type NewView = carnot_engine::NewView<[u8; 32]>;
