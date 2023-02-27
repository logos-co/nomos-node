use crate::node::StepTime;
use std::collections::HashMap;

pub enum CarnotStep {}

pub struct CarnotNodeSettings {
    steps_costs: HashMap<CarnotStep, StepTime>,
}
pub struct CarnotNode {
    id: usize,
    settings: CarnotSettings,
    state: CarnotState,
}
