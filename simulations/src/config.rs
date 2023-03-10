use crate::network::regions::Region;
use crate::node::StepTime;
use crate::{node::Node, overlay::Overlay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

// TODO: manully implement ser/deser
// #[derive(Serialize, Deserialize)]
pub struct Config<N: Node, O: Overlay> {
    pub network_behaviors: HashMap<(Region, Region), StepTime>,
    pub regions: Vec<Region>,
    pub overlay_settings: O::Settings,
    pub node_settings: N::Settings,
    pub node_count: usize,
    pub committee_size: usize,
}

impl<N: Node, O: Overlay> serde::Serialize for Config<N, O> {
    fn serialize<S>(&self, _serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        todo!()
    }
}

impl<'de, N: Node, O: Overlay> serde::Deserialize<'de> for Config<N, O> {
    fn deserialize<D>(_deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        todo!()
    }
}
