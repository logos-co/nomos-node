use super::*;

const NODE_ID: &str = "node_id";
const CURRENT_VIEW: &str = "current_view";
const HIGHEST_VOTED_VIEW: &str = "highest_voted_view";
const LOCAL_HIGH_QC: &str = "local_high_qc";
const SAFE_BLOCKS: &str = "safe_blocks";
const LAST_VIEW_TIMEOUT_QC: &str = "last_view_timeout_qc";
const LATEST_COMMITTED_BLOCK: &str = "latest_committed_block";
const LATEST_COMMITTED_VIEW: &str = "latest_committed_view";
const ROOT_COMMITTEE: &str = "root_committee";
const PARENT_COMMITTEE: &str = "parent_committee";
const CHILD_COMMITTEES: &str = "child_committees";
const COMMITTED_BLOCKS: &str = "committed_blocks";
const STEP_DURATION: &str = "step_duration";

pub const CARNOT_RECORD_KEYS: &[&str] = &[
    NODE_ID,
    CURRENT_VIEW,
    HIGHEST_VOTED_VIEW,
    LOCAL_HIGH_QC,
    SAFE_BLOCKS,
    LAST_VIEW_TIMEOUT_QC,
    LATEST_COMMITTED_BLOCK,
    LATEST_COMMITTED_VIEW,
    ROOT_COMMITTEE,
    PARENT_COMMITTEE,
    CHILD_COMMITTEES,
    COMMITTED_BLOCKS,
    STEP_DURATION,
];

#[derive(Debug, Clone)]
pub struct CarnotState {
    node_id: NodeId,
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    latest_committed_block: Block,
    latest_committed_view: View,
    root_committee: Committee,
    parent_committee: Option<Committee>,
    child_committees: Vec<Committee>,
    committed_blocks: Vec<BlockId>,
    pub(super) step_duration: Duration,

    /// does not serialize this field, this field is used to check
    /// how to serialize other fields because csv format does not support
    /// nested map or struct, we have to do some customize.
    pub(super) format: SubscriberFormat,
}

#[derive(Serialize)]
#[serde(untagged)]
pub enum CarnotRecord {
    Runtime(Runtime),
    Settings(Box<SimulationSettings>),
    Data(Vec<Box<CarnotState>>),
}

impl From<Runtime> for CarnotRecord {
    fn from(value: Runtime) -> Self {
        Self::Runtime(value)
    }
}

impl From<SimulationSettings> for CarnotRecord {
    fn from(value: SimulationSettings) -> Self {
        Self::Settings(Box::new(value))
    }
}

impl Record for CarnotRecord {
    type Data = CarnotState;

    fn record_type(&self) -> RecordType {
        match self {
            CarnotRecord::Runtime(_) => RecordType::Meta,
            CarnotRecord::Settings(_) => RecordType::Settings,
            CarnotRecord::Data(_) => RecordType::Data,
        }
    }

    fn fields(&self) -> Vec<&str> {
        let mut fields = RECORD_SETTINGS
            .get()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>();
        // sort fields to make sure the we are matching header field and record field when using csv format
        fields.sort();
        fields
    }

    fn data(&self) -> Vec<&CarnotState> {
        match self {
            CarnotRecord::Data(d) => d.iter().map(AsRef::as_ref).collect(),
            _ => vec![],
        }
    }
}

impl<S, T: Clone + Serialize + 'static> TryFrom<&SimulationState<S, T>> for CarnotRecord {
    type Error = anyhow::Error;

    fn try_from(state: &SimulationState<S, T>) -> Result<Self, Self::Error> {
        let Ok(states) = state
            .nodes
            .read()
            .iter()
            .map(|n| Box::<dyn Any + 'static>::downcast(Box::new(n.state().clone())))
            .collect::<Result<Vec<_>, _>>()
        else {
            return Err(anyhow::anyhow!("use carnot record on other node"));
        };
        Ok(Self::Data(states))
    }
}

impl serde::Serialize for CarnotState {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        use serde::ser::SerializeStruct;
        if let Some(rs) = RECORD_SETTINGS.get() {
            let mut keys = rs
                .iter()
                .filter_map(|(k, v)| {
                    if CARNOT_RECORD_KEYS.contains(&k.trim()) && *v {
                        Some(k)
                    } else {
                        None
                    }
                })
                .collect::<Vec<_>>();

            // sort keys to make sure the we are matching header field and record field when using csv format
            keys.sort();

            let mut ser = serializer.serialize_struct("CarnotState", keys.len())?;
            for k in keys {
                match k.trim() {
                    NODE_ID => ser.serialize_field(NODE_ID, &self.node_id.index())?,
                    CURRENT_VIEW => ser.serialize_field(CURRENT_VIEW, &self.current_view)?,
                    HIGHEST_VOTED_VIEW => {
                        ser.serialize_field(HIGHEST_VOTED_VIEW, &self.highest_voted_view)?
                    }
                    LOCAL_HIGH_QC => {
                        if self.format.is_csv() {
                            ser.serialize_field(
                                LOCAL_HIGH_QC,
                                &format!(
                                    "{{view: {}, block: {}}}",
                                    self.local_high_qc.view, self.local_high_qc.id
                                ),
                            )?
                        } else {
                            ser.serialize_field(LOCAL_HIGH_QC, &self.local_high_qc)?
                        }
                    }
                    SAFE_BLOCKS => {
                        if self.format == SubscriberFormat::Csv {
                            #[derive(Serialize)]
                            #[serde(transparent)]
                            struct SafeBlockHelper<'a> {
                                #[serde(serialize_with = "serialize_blocks_to_csv")]
                                safe_blocks: &'a HashMap<BlockId, Block>,
                            }
                            ser.serialize_field(
                                SAFE_BLOCKS,
                                &SafeBlockHelper {
                                    safe_blocks: &self.safe_blocks,
                                },
                            )?;
                        } else {
                            #[derive(Serialize)]
                            #[serde(transparent)]
                            struct SafeBlockHelper<'a> {
                                #[serde(serialize_with = "serialize_blocks_to_json")]
                                safe_blocks: &'a HashMap<BlockId, Block>,
                            }
                            ser.serialize_field(
                                SAFE_BLOCKS,
                                &SafeBlockHelper {
                                    safe_blocks: &self.safe_blocks,
                                },
                            )?;
                        }
                    }
                    LAST_VIEW_TIMEOUT_QC => {
                        if self.format.is_csv() {
                            ser.serialize_field(
                                LAST_VIEW_TIMEOUT_QC,
                                &self.last_view_timeout_qc.as_ref().map(|tc| {
                                    let qc = tc.high_qc();
                                    format!(
                                        "{{view: {}, standard_view: {}, block: {}, node: {}}}",
                                        tc.view(),
                                        qc.view,
                                        qc.id,
                                        tc.sender().index()
                                    )
                                }),
                            )?
                        } else {
                            ser.serialize_field(LAST_VIEW_TIMEOUT_QC, &self.last_view_timeout_qc)?
                        }
                    }
                    LATEST_COMMITTED_BLOCK => ser.serialize_field(
                        LATEST_COMMITTED_BLOCK,
                        &block_to_csv_field(&self.latest_committed_block)
                            .map_err(<S::Error as serde::ser::Error>::custom)?,
                    )?,
                    LATEST_COMMITTED_VIEW => {
                        ser.serialize_field(LATEST_COMMITTED_VIEW, &self.latest_committed_view)?
                    }
                    ROOT_COMMITTEE => {
                        if self.format.is_csv() {
                            let val = self
                                .root_committee
                                .iter()
                                .map(|n| n.index().to_string())
                                .collect::<Vec<_>>();
                            ser.serialize_field(ROOT_COMMITTEE, &format!("[{}]", val.join(",")))?
                        } else {
                            ser.serialize_field(
                                ROOT_COMMITTEE,
                                &self
                                    .root_committee
                                    .iter()
                                    .map(|n| n.index())
                                    .collect::<Vec<_>>(),
                            )?
                        }
                    }
                    PARENT_COMMITTEE => {
                        if self.format.is_csv() {
                            let committees = self.parent_committee.as_ref().map(|c| {
                                c.iter().map(|n| n.index().to_string()).collect::<Vec<_>>()
                            });
                            let committees = committees.map(|c| {
                                let c = c.join(",");
                                format!("[{c}]")
                            });
                            ser.serialize_field(CHILD_COMMITTEES, &committees)?
                        } else {
                            let committees = self
                                .parent_committee
                                .as_ref()
                                .map(|c| c.iter().map(|n| n.index()).collect::<Vec<_>>());
                            ser.serialize_field(PARENT_COMMITTEE, &committees)?
                        }
                    }
                    CHILD_COMMITTEES => {
                        let committees = self
                            .child_committees
                            .iter()
                            .map(|c| {
                                let s = c
                                    .iter()
                                    .map(|n| n.index().to_string())
                                    .collect::<Vec<_>>()
                                    .join(",");
                                format!("[{s}]")
                            })
                            .collect::<Vec<_>>();
                        if self.format.is_csv() {
                            let committees = committees.join(",");
                            ser.serialize_field(CHILD_COMMITTEES, &format!("[{committees}]"))?
                        } else {
                            ser.serialize_field(CHILD_COMMITTEES, &committees)?
                        }
                    }
                    COMMITTED_BLOCKS => {
                        let blocks = self
                            .committed_blocks
                            .iter()
                            .map(|b| b.to_string())
                            .collect::<Vec<_>>();
                        if self.format.is_csv() {
                            let blocks = blocks.join(",");
                            ser.serialize_field(COMMITTED_BLOCKS, &format!("[{blocks}]"))?
                        } else {
                            ser.serialize_field(COMMITTED_BLOCKS, &blocks)?
                        }
                    }
                    STEP_DURATION => ser.serialize_field(
                        STEP_DURATION,
                        &humantime::format_duration(self.step_duration).to_string(),
                    )?,
                    _ => {}
                }
            }
            ser.end()
        } else {
            serializer.serialize_none()
        }
    }
}

impl CarnotState {
    const fn keys() -> &'static [&'static str] {
        CARNOT_RECORD_KEYS
    }
}

/// Have to implement this manually because of the `serde_json` will panic if the key of map
/// is not a string.
fn serialize_blocks_to_json<S>(
    blocks: &HashMap<BlockId, Block>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    use serde::ser::SerializeMap;
    let mut ser = serializer.serialize_map(Some(blocks.len()))?;
    for (k, v) in blocks {
        ser.serialize_entry(&format!("{k}"), v)?;
    }
    ser.end()
}

fn serialize_blocks_to_csv<S>(
    blocks: &HashMap<BlockId, Block>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    let v = blocks
        .values()
        .map(block_to_csv_field)
        .collect::<Result<Vec<_>, _>>()
        .map_err(<S::Error as serde::ser::Error>::custom)?
        .join(",");
    serializer.serialize_str(&format!("[{v}]"))
}

fn block_to_csv_field(v: &Block) -> serde_json::Result<String> {
    struct Helper<'a>(&'a Block);

    impl<'a> serde::Serialize for Helper<'a> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: serde::Serializer,
        {
            use serde::ser::SerializeStruct;
            let mut s = serializer.serialize_struct("Block", 4)?;
            s.serialize_field("id", &self.0.id.to_string())?;
            s.serialize_field("view", &self.0.view)?;
            s.serialize_field("parent_qc", &self.0.parent_qc)?;
            s.serialize_field("leader_proof", &self.0.leader_proof)?;
            s.end()
        }
    }
    serde_json::to_string(&Helper(v))
}

impl<O: Overlay> From<&Carnot<O>> for CarnotState {
    fn from(value: &Carnot<O>) -> Self {
        let node_id = value.id();
        let current_view = value.current_view();
        Self {
            node_id,
            current_view,
            local_high_qc: value.high_qc(),
            parent_committee: value.parent_committee(),
            root_committee: value.root_committee(),
            child_committees: value.child_committees(),
            latest_committed_block: value.latest_committed_block(),
            latest_committed_view: value.latest_committed_view(),
            safe_blocks: value
                .blocks_in_view(current_view)
                .into_iter()
                .map(|b| (b.id, b))
                .collect(),
            last_view_timeout_qc: value.last_view_timeout_qc(),
            committed_blocks: value.latest_committed_blocks(),
            highest_voted_view: Default::default(),
            step_duration: Default::default(),
            format: SubscriberFormat::Csv,
        }
    }
}
