use crate::network::regions::Region;
use crate::node::StepTime;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Serialize, Deserialize)]
pub struct Config<N, O, S, C>
where
    S: core::str::FromStr,
    C: core::str::FromStr,
{
    pub network_behaviors: HashMap<(Region, Region), StepTime>,
    pub regions: Vec<Region>,
    pub overlay_settings: O,
    pub node_settings: N,
    pub node_count: usize,
    pub committee_size: usize,
    pub step_costs: Vec<(S, C)>,
}
