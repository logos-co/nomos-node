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

#[serde_with::skip_serializing_none]
#[serde_with::serde_as]
#[derive(Serialize, Default)]
pub(crate) struct CarnotState<'a> {
    pub(crate) node_id: Option<NodeIdHelper>,
    pub(crate) current_view: Option<View>,
    pub(crate) highest_voted_view: Option<View>,
    pub(crate) local_high_qc: Option<StandardQcHelper>,
    pub(crate) safe_blocks: Option<SafeBlocksHelper<'a>>,
    pub(crate) last_view_timeout_qc: Option<Option<TimeoutQcHelper<'a>>>,
    pub(crate) latest_committed_block: Option<BlockHelper>,
    pub(crate) latest_committed_view: Option<View>,
    pub(crate) root_committee: Option<CommitteeHelper<'a>>,
    pub(crate) parent_committee: Option<Option<CommitteeHelper<'a>>>,
    pub(crate) child_committees: Option<CommitteesHelper<'a>>,
    pub(crate) committed_blocks: Option<CommittedBlockHelper<'a>>,
    #[serde_as(as = "Option<serde_with::DurationMilliSeconds>")]
    pub(crate) step_duration: Option<Duration>,
}

impl<'a> From<&'a super::CarnotState> for CarnotState<'a> {
    fn from(value: &'a super::CarnotState) -> Self {
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
    use super::*;

    #[derive(Serialize)]
    pub(crate) struct BlockHelper {
        view: View,
        id: BlockIdHelper,
    }

    impl From<&Block> for BlockHelper {
        fn from(val: &Block) -> Self {
            Self {
                view: val.view,
                id: val.id.into(),
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
