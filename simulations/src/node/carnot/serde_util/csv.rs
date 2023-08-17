use super::*;

#[serde_with::skip_serializing_none]
#[serde_with::serde_as]
#[derive(Serialize, Default)]
pub(crate) struct CarnotStateCsvSerializer<'a> {
    node_id: Option<NodeIdHelper>,
    current_view: Option<View>,
    highest_voted_view: Option<View>,
    local_high_qc: Option<StandardQcHelper>,
    safe_blocks: Option<SafeBlocksHelper<'a>>,
    last_view_timeout_qc: Option<Option<TimeoutQcHelper<'a>>>,
    latest_committed_block: Option<BlockHelper<'a>>,
    latest_committed_view: Option<View>,
    root_committee: Option<CommitteeHelper<'a>>,
    parent_committee: Option<Option<CommitteeHelper<'a>>>,
    child_committees: Option<CommitteesHelper<'a>>,
    committed_blocks: Option<CommittedBlockHelper<'a>>,
    #[serde_as(as = "Option<serde_with::DurationMilliSeconds>")]
    step_duration: Option<Duration>,
}

impl<'a> CarnotStateCsvSerializer<'a> {
    pub(crate) fn serialize_state<S: serde::ser::Serializer>(
        &mut self,
        keys: Vec<&String>,
        state: &'a super::super::CarnotState,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        for k in keys {
            match k.trim() {
                NODE_ID => {
                    self.node_id = Some(state.node_id.into());
                }
                CURRENT_VIEW => {
                    self.current_view = Some(state.current_view);
                }
                HIGHEST_VOTED_VIEW => {
                    self.highest_voted_view = Some(state.highest_voted_view);
                }
                LOCAL_HIGH_QC => {
                    self.local_high_qc = Some((&state.local_high_qc).into());
                }
                SAFE_BLOCKS => {
                    self.safe_blocks = Some((&state.safe_blocks).into());
                }
                LAST_VIEW_TIMEOUT_QC => {
                    self.last_view_timeout_qc =
                        Some(state.last_view_timeout_qc.as_ref().map(From::from));
                }
                LATEST_COMMITTED_BLOCK => {
                    self.latest_committed_block = Some((&state.latest_committed_block).into());
                }
                LATEST_COMMITTED_VIEW => {
                    self.latest_committed_view = Some(state.latest_committed_view);
                }
                ROOT_COMMITTEE => {
                    self.root_committee = Some((&state.root_committee).into());
                }
                PARENT_COMMITTEE => {
                    self.parent_committee = Some(state.parent_committee.as_ref().map(From::from));
                }
                CHILD_COMMITTEES => {
                    self.child_committees = Some(state.child_committees.as_slice().into());
                }
                COMMITTED_BLOCKS => {
                    self.committed_blocks = Some(state.committed_blocks.as_slice().into());
                }
                STEP_DURATION => {
                    self.step_duration = Some(state.step_duration);
                }
                _ => {}
            }
        }
        state.serialize(serializer)
    }
}

struct SafeBlocksHelper<'a>(&'a HashMap<BlockId, Block>);

impl<'a> From<&'a HashMap<BlockId, Block>> for SafeBlocksHelper<'a> {
    fn from(val: &'a HashMap<BlockId, Block>) -> Self {
        Self(val)
    }
}

impl<'a> Serialize for SafeBlocksHelper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0
            .values()
            .map(|b| serde_json::to_string(&BlockHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}

struct CommitteeHelper<'a>(&'a Committee);

impl<'a> From<&'a Committee> for CommitteeHelper<'a> {
    fn from(val: &'a Committee) -> Self {
        Self(val)
    }
}

impl<'a> Serialize for CommitteeHelper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0
            .iter()
            .map(|&b| serde_json::to_string(&NodeIdHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}

struct CommitteesHelper<'a>(&'a [Committee]);

impl<'a> From<&'a [Committee]> for CommitteesHelper<'a> {
    fn from(val: &'a [Committee]) -> Self {
        Self(val)
    }
}

impl<'a> Serialize for CommitteesHelper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0
            .iter()
            .map(|b| serde_json::to_string(&CommitteeHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}

struct CommittedBlockHelper<'a>(&'a [BlockId]);

impl<'a> From<&'a [BlockId]> for CommittedBlockHelper<'a> {
    fn from(val: &'a [BlockId]) -> Self {
        Self(val)
    }
}

impl<'a> Serialize for CommittedBlockHelper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0
            .iter()
            .map(|&b| serde_json::to_string(&BlockIdHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}
