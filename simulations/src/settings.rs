use crate::network::regions::Region;
use crate::node::StepTime;
use crate::overlay::tree::TreeSettings;
use crate::streaming::io::IOStreamSettings;
use crate::streaming::StreamSettings;
use crate::warding::Ward;
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Clone, Debug, Deserialize, Default)]
pub enum RunnerSettings {
    #[default]
    Sync,
    Async {
        chunks: usize,
    },
    Glauber {
        maximum_iterations: usize,
        update_rate: usize,
    },
    Layered {
        rounds_gap: usize,
        distribution: Option<Vec<f32>>,
    },
}

#[derive(Clone, Debug, Deserialize, Default)]
pub enum NodeSettings {
    Carnot,
    #[default]
    Dummy,
}

#[derive(Clone, Debug, Deserialize)]
pub enum OverlaySettings {
    Flat,
    Tree(TreeSettings),
}

impl Default for OverlaySettings {
    fn default() -> Self {
        Self::Tree(Default::default())
    }
}

impl From<TreeSettings> for OverlaySettings {
    fn from(settings: TreeSettings) -> OverlaySettings {
        OverlaySettings::Tree(settings)
    }
}

impl TryInto<TreeSettings> for OverlaySettings {
    type Error = String;

    fn try_into(self) -> Result<TreeSettings, Self::Error> {
        if let Self::Tree(settings) = self {
            Ok(settings)
        } else {
            Err("unable to convert to tree settings".into())
        }
    }
}

impl<W> TryInto<IOStreamSettings<W>> for StreamSettings {
    type Error = String;

    fn try_into(self) -> Result<IOStreamSettings<W>, Self::Error> {
        match self {
            StreamSettings::IO(settings) => Ok(IOStreamSettings::<W> { writer: todo!() }),
            _ => Err("".into()),
        }
    }

    // fn try_from(settings: StreamSettings) -> Result<Self, Self::Error> {
    //     match settings {
    //         StreamSettings::IO(settings) => Ok(IOStreamSettings {
    //             writer: settings.writer,
    //         }),
    //         _ => Err("io settings can't be created".into()),
    //     }
    // }
}

#[derive(Default, Deserialize)]
pub struct SimulationSettings {
    pub network_behaviors: HashMap<(Region, Region), StepTime>,
    pub regions: Vec<Region>,
    #[serde(default)]
    pub wards: Vec<Ward>,
    pub overlay_settings: OverlaySettings,
    pub node_settings: NodeSettings,
    pub runner_settings: RunnerSettings,
    pub stream_settings: StreamSettings,
    pub node_count: usize,
    pub committee_size: usize,
    pub seed: Option<u64>,
}
