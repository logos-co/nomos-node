use crate::network::regions::Region;
use crate::node::StepTime;
use crate::{node::Node, overlay::Overlay};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

pub struct Config<N: Node, O: Overlay> {
    pub network_behaviors: HashMap<(Region, Region), StepTime>,
    pub regions: Vec<Region>,
    pub overlay_settings: O::Settings,
    pub node_settings: N::Settings,
    pub node_count: usize,
    pub committee_size: usize,
}

impl<N, O> serde::Serialize for Config<N, O>
where
    N: Node,
    N::Settings: Serialize,
    O: Overlay,
    O::Settings: Serialize,
{
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        let mut ser = serializer.serialize_struct(core::any::type_name::<Self>(), 6)?;
        ser.serialize_field("network_behaviors", &self.network_behaviors)?;
        ser.serialize_field("regions", &self.regions)?;
        ser.serialize_field("overlay_settings", &self.overlay_settings)?;
        ser.serialize_field("node_settings", &self.node_settings)?;
        ser.serialize_field("node_count", &self.node_count)?;
        ser.serialize_field("committee_size", &self.committee_size)?;
        ser.end()
    }
}

impl<'de, N: Node, O: Overlay> serde::Deserialize<'de> for Config<N, O>
where
    N: Node,
    N::Settings: Deserialize<'de>,
    O: Overlay,
    O::Settings: Deserialize<'de>,
{
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        use serde::de::{self, Visitor};

        struct ConfigVisitor<N: Node, O: Overlay> {
            _marker: std::marker::PhantomData<fn() -> (N, O)>,
        }

        impl<N: Node, O: Overlay> Default for ConfigVisitor<N, O> {
            fn default() -> Self {
                Self {
                    _marker: std::marker::PhantomData,
                }
            }
        }

        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "snake_case")]
        enum Field {
            NetworkBehaviors,
            Regions,
            OverlaySettings,
            NodeSettings,
            NodeCount,
            CommitteeSize,
        }

        impl<'de, N, O> Visitor<'de> for ConfigVisitor<N, O>
        where
            N: Node,
            N::Settings: Deserialize<'de>,
            O: Overlay,
            O::Settings: Deserialize<'de>,
        {
            type Value = Config<N, O>;

            fn expecting(&self, formatter: &mut std::fmt::Formatter) -> std::fmt::Result {
                formatter.write_str(&format!("struct {}", core::any::type_name::<Self>()))
            }

            fn visit_map<A>(self, mut map: A) -> Result<Self::Value, A::Error>
            where
                A: serde::de::MapAccess<'de>,
            {
                let mut network_behaviors = None;
                let mut regions = None;
                let mut overlay_settings = None;
                let mut node_settings = None;
                let mut node_count = None;
                let mut committee_size = None;
                while let Some(key) = map.next_key()? {
                    match key {
                        Field::NetworkBehaviors => {
                            if network_behaviors.is_some() {
                                return Err(de::Error::duplicate_field("network_behaviors"));
                            }
                            network_behaviors = Some(map.next_value()?);
                        }
                        Field::Regions => {
                            if regions.is_some() {
                                return Err(de::Error::duplicate_field("regions"));
                            }
                            regions = Some(map.next_value()?);
                        }
                        Field::OverlaySettings => {
                            if overlay_settings.is_some() {
                                return Err(de::Error::duplicate_field("overlay_settings"));
                            }
                            overlay_settings = Some(map.next_value()?);
                        }
                        Field::NodeSettings => {
                            if node_settings.is_some() {
                                return Err(de::Error::duplicate_field("node_settings"));
                            }
                            node_settings = Some(map.next_value()?);
                        }
                        Field::NodeCount => {
                            if node_count.is_some() {
                                return Err(de::Error::duplicate_field("node_count"));
                            }
                            node_count = Some(map.next_value()?);
                        }
                        Field::CommitteeSize => {
                            if committee_size.is_some() {
                                return Err(de::Error::duplicate_field("committee_size"));
                            }
                            committee_size = Some(map.next_value()?);
                        }
                    }
                }
                let network_behaviors = network_behaviors
                    .ok_or_else(|| de::Error::missing_field("network_behaviours"))?;
                let regions = regions.ok_or_else(|| de::Error::missing_field("regions"))?;
                let overlay_settings =
                    overlay_settings.ok_or_else(|| de::Error::missing_field("overlay_settings"))?;
                let node_settings =
                    node_settings.ok_or_else(|| de::Error::missing_field("node_settings"))?;
                let node_count =
                    node_count.ok_or_else(|| de::Error::missing_field("node_count"))?;
                let committee_size =
                    committee_size.ok_or_else(|| de::Error::missing_field("committee_size"))?;
                Ok(Config {
                    network_behaviors,
                    regions,
                    overlay_settings,
                    node_settings,
                    node_count,
                    committee_size,
                })
            }
        }

        const FIELDS: &[&str] = &[
            "network_behaviors",
            "regions",
            "overlay_settings",
            "node_settings",
            "node_count",
            "committee_size",
        ];
        deserializer.deserialize_struct(
            core::any::type_name::<Self>(),
            FIELDS,
            ConfigVisitor::default(),
        )
    }
}
