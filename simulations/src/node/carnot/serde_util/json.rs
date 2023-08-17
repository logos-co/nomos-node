use super::*;

#[serde_with::skip_serializing_none]
#[serde_with::serde_as]
#[derive(Serialize, Default)]
pub(crate) struct CarnotStateJsonSerializer<'a> {
    pub(crate) node_id: Option<NodeIdHelper>,
    pub(crate) current_view: Option<View>,
    pub(crate) highest_voted_view: Option<View>,
    pub(crate) local_high_qc: Option<StandardQcHelper>,
    pub(crate) safe_blocks: Option<SafeBlocksHelper<'a>>,
    pub(crate) last_view_timeout_qc: Option<Option<TimeoutQcHelper<'a>>>,
    pub(crate) latest_committed_block: Option<BlockHelper<'a>>,
    pub(crate) latest_committed_view: Option<View>,
    pub(crate) root_committee: Option<CommitteeHelper<'a>>,
    pub(crate) parent_committee: Option<Option<CommitteeHelper<'a>>>,
    pub(crate) child_committees: Option<CommitteesHelper<'a>>,
    pub(crate) committed_blocks: Option<CommittedBlockHelper<'a>>,
    #[serde_as(as = "Option<serde_with::DurationMilliSeconds>")]
    pub(crate) step_duration: Option<Duration>,
}

impl<'a> CarnotStateJsonSerializer<'a> {
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

impl<'a> From<&'a super::super::CarnotState> for CarnotStateJsonSerializer<'a> {
    fn from(value: &'a super::super::CarnotState) -> Self {
        Self {
            node_id: Some(value.node_id.into()),
            current_view: Some(value.current_view),
            highest_voted_view: Some(value.highest_voted_view),
            local_high_qc: Some(StandardQcHelper::from(&value.local_high_qc)),
            safe_blocks: Some(SafeBlocksHelper::from(&value.safe_blocks)),
            last_view_timeout_qc: Some(value.last_view_timeout_qc.as_ref().map(From::from)),
            latest_committed_block: Some(BlockHelper::from(&value.latest_committed_block)),
            latest_committed_view: Some(value.latest_committed_view),
            root_committee: Some(CommitteeHelper::from(&value.root_committee)),
            parent_committee: Some(value.parent_committee.as_ref().map(From::from)),
            child_committees: Some(CommitteesHelper::from(value.child_committees.as_slice())),
            committed_blocks: Some(CommittedBlockHelper::from(
                value.committed_blocks.as_slice(),
            )),
            step_duration: Some(value.step_duration),
        }
    }
}

pub(crate) struct SafeBlocksHelper<'a>(&'a HashMap<BlockId, Block>);

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
        let iter = self.0.values();
        let mut s = serializer.serialize_seq(Some(iter.size_hint().0))?;
        for b in iter {
            s.serialize_element(&BlockHelper::from(b))?;
        }
        s.end()
    }
}

pub(crate) struct CommitteeHelper<'a>(&'a Committee);

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
        let iter = self.0.iter();
        let mut s = serializer.serialize_seq(Some(iter.size_hint().0))?;
        for id in iter {
            s.serialize_element(&NodeIdHelper::from(*id))?;
        }
        s.end()
    }
}

pub(crate) struct CommitteesHelper<'a>(&'a [Committee]);

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
        let mut s = serializer.serialize_seq(Some(self.0.len()))?;
        for c in self.0 {
            s.serialize_element(&CommitteeHelper::from(c))?;
        }
        s.end()
    }
}

pub(crate) struct CommittedBlockHelper<'a>(&'a [BlockId]);

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
        let mut s = serializer.serialize_seq(Some(self.0.len()))?;
        for c in self.0 {
            s.serialize_element(&BlockIdHelper::from(*c))?;
        }
        s.end()
    }
}
