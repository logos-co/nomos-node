use std::{collections::HashMap, time::Duration};

use serde::{
    ser::{SerializeSeq, Serializer},
    Deserialize, Serialize,
};

use self::{
    serde_block::BlockHelper,
    serde_id::{BlockIdHelper, NodeIdHelper},
    standard_qc::StandardQcHelper,
    timeout_qc::TimeoutQcHelper,
};
use consensus_engine::{AggregateQc, Block, BlockId, Committee, Qc, StandardQc, TimeoutQc, View};

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

macro_rules! serializer {
    ($name: ident) => {
        #[serde_with::skip_serializing_none]
        #[serde_with::serde_as]
        #[derive(Serialize, Default)]
        pub(crate) struct $name<'a> {
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

        impl<'a> $name<'a> {
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
                            self.latest_committed_block =
                                Some((&state.latest_committed_block).into());
                        }
                        LATEST_COMMITTED_VIEW => {
                            self.latest_committed_view = Some(state.latest_committed_view);
                        }
                        ROOT_COMMITTEE => {
                            self.root_committee = Some((&state.root_committee).into());
                        }
                        PARENT_COMMITTEE => {
                            self.parent_committee =
                                Some(state.parent_committee.as_ref().map(From::from));
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
    };
}

mod csv;
mod json;

pub(super) use self::csv::CarnotStateCsvSerializer;
pub(super) use json::CarnotStateJsonSerializer;

pub(crate) mod standard_qc {
    use super::*;

    #[derive(Serialize)]
    pub(crate) struct StandardQcHelper {
        view: View,
        id: serde_id::BlockIdHelper,
    }

    impl From<&StandardQc> for StandardQcHelper {
        fn from(val: &StandardQc) -> Self {
            Self {
                view: val.view,
                id: val.id.into(),
            }
        }
    }

    pub fn serialize<S: Serializer>(t: &StandardQc, serializer: S) -> Result<S::Ok, S::Error> {
        StandardQcHelper::from(t).serialize(serializer)
    }
}

pub(crate) mod aggregate_qc {
    use super::*;

    #[derive(Serialize)]
    pub(crate) struct AggregateQcHelper<'a> {
        #[serde(serialize_with = "standard_qc::serialize")]
        high_qc: &'a StandardQc,
        view: View,
    }

    impl<'a> From<&'a AggregateQc> for AggregateQcHelper<'a> {
        fn from(t: &'a AggregateQc) -> Self {
            Self {
                high_qc: &t.high_qc,
                view: t.view,
            }
        }
    }

    pub fn serialize<S: serde::Serializer>(
        t: &AggregateQc,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        AggregateQcHelper::from(t).serialize(serializer)
    }
}

pub(crate) mod qc {
    use super::*;

    #[derive(Serialize)]
    #[serde(untagged)]
    pub(crate) enum QcHelper<'a> {
        Standard(#[serde(serialize_with = "standard_qc::serialize")] &'a StandardQc),
        Aggregate(aggregate_qc::AggregateQcHelper<'a>),
    }

    pub fn serialize<S: serde::Serializer>(t: &Qc, serializer: S) -> Result<S::Ok, S::Error> {
        let qc = match t {
            Qc::Standard(s) => QcHelper::Standard(s),
            Qc::Aggregated(a) => QcHelper::Aggregate(aggregate_qc::AggregateQcHelper::from(a)),
        };
        qc.serialize(serializer)
    }
}

pub(crate) mod timeout_qc {
    use super::*;
    use consensus_engine::NodeId;

    #[derive(Serialize)]
    pub(crate) struct TimeoutQcHelper<'a> {
        view: View,
        #[serde(serialize_with = "standard_qc::serialize")]
        high_qc: &'a StandardQc,
        #[serde(serialize_with = "serde_id::serialize_node_id")]
        sender: NodeId,
    }

    impl<'a> From<&'a TimeoutQc> for TimeoutQcHelper<'a> {
        fn from(value: &'a TimeoutQc) -> Self {
            Self {
                view: value.view(),
                high_qc: value.high_qc(),
                sender: value.sender(),
            }
        }
    }

    pub fn serialize<S: serde::Serializer>(
        t: &TimeoutQc,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        TimeoutQcHelper::from(t).serialize(serializer)
    }
}

pub(crate) mod serde_block {
    use consensus_engine::LeaderProof;

    use super::*;

    #[derive(Serialize)]
    #[serde(untagged)]
    enum LeaderProofHelper {
        LeaderId { leader_id: NodeIdHelper },
    }

    impl From<&LeaderProof> for LeaderProofHelper {
        fn from(value: &LeaderProof) -> Self {
            match value {
                LeaderProof::LeaderId { leader_id } => Self::LeaderId {
                    leader_id: (*leader_id).into(),
                },
            }
        }
    }

    #[derive(Serialize)]
    pub(crate) struct BlockHelper<'a> {
        view: View,
        id: BlockIdHelper,
        parent_qc: &'a Qc,
        leader_proof: LeaderProofHelper,
    }

    impl<'a> From<&'a Block> for BlockHelper<'a> {
        fn from(val: &'a Block) -> Self {
            Self {
                view: val.view,
                id: val.id.into(),
                parent_qc: &val.parent_qc,
                leader_proof: (&val.leader_proof).into(),
            }
        }
    }

    pub fn serialize<S: serde::Serializer>(t: &Block, serializer: S) -> Result<S::Ok, S::Error> {
        BlockHelper::from(t).serialize(serializer)
    }
}

pub(crate) mod serde_id {
    use consensus_engine::{BlockId, NodeId};

    use super::*;

    #[derive(Serialize, Deserialize)]
    pub(crate) struct BlockIdHelper(#[serde(with = "serde_array32")] [u8; 32]);

    impl From<BlockId> for BlockIdHelper {
        fn from(val: BlockId) -> Self {
            Self(val.into())
        }
    }

    #[derive(Serialize, Deserialize)]
    pub(crate) struct NodeIdHelper(#[serde(with = "serde_array32")] [u8; 32]);

    impl From<NodeId> for NodeIdHelper {
        fn from(val: NodeId) -> Self {
            Self(val.into())
        }
    }

    pub fn serialize_node_id<S: serde::Serializer>(
        t: &NodeId,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        NodeIdHelper::from(*t).serialize(serializer)
    }

    pub(crate) mod serde_array32 {
        use super::*;
        use std::cell::RefCell;

        const MAX_SERIALIZATION_LENGTH: usize = 32 * 2 + 2;

        thread_local! {
            static STRING_BUFFER: RefCell<String> = RefCell::new(String::with_capacity(MAX_SERIALIZATION_LENGTH));
        }

        pub fn serialize<S: serde::Serializer>(
            t: &[u8; 32],
            serializer: S,
        ) -> Result<S::Ok, S::Error> {
            if serializer.is_human_readable() {
                STRING_BUFFER.with(|s| {
                    let mut s = s.borrow_mut();
                    s.clear();
                    s.push_str("0x");
                    for v in t {
                        std::fmt::write(&mut *s, format_args!("{:02x}", v)).unwrap();
                    }
                    s.serialize(serializer)
                })
            } else {
                t.serialize(serializer)
            }
        }

        pub fn deserialize<'de, D>(deserializer: D) -> Result<[u8; 32], D::Error>
        where
            D: serde::Deserializer<'de>,
        {
            if deserializer.is_human_readable() {
                <&str>::deserialize(deserializer).and_then(|s| {
                    super::parse_hex_from_str::<32>(s)
                        .map_err(<D::Error as serde::de::Error>::custom)
                })
            } else {
                let x = <&[u8]>::deserialize(deserializer)?;
                <[u8; 32]>::try_from(x).map_err(<D::Error as serde::de::Error>::custom)
            }
        }
    }

    #[derive(Debug, thiserror::Error)]
    enum DecodeError {
        #[error("expected str of length {expected} but got length {actual}")]
        UnexpectedSize { expected: usize, actual: usize },
        #[error("invalid character pair '{0}{1}'")]
        InvalidCharacter(char, char),
    }

    fn parse_hex_from_str<const N: usize>(mut s: &str) -> Result<[u8; N], DecodeError> {
        // check if we start with 0x or not
        let prefix_len = if s.starts_with("0x") || s.starts_with("0X") {
            s = &s[2..];
            2
        } else {
            0
        };

        if s.len() != N * 2 {
            return Err(DecodeError::UnexpectedSize {
                expected: N * 2 + prefix_len,
                actual: s.len(),
            });
        }

        let mut output = [0; N];

        for (chars, byte) in s.as_bytes().chunks_exact(2).zip(output.iter_mut()) {
            let (l, r) = (chars[0] as char, chars[1] as char);
            match (l.to_digit(16), r.to_digit(16)) {
                (Some(l), Some(r)) => *byte = (l as u8) << 4 | r as u8,
                (_, _) => return Err(DecodeError::InvalidCharacter(l, r)),
            };
        }
        Ok(output)
    }
}
