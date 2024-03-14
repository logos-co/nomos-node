use super::*;
use nomos_core::header::HeaderId;
use serde_block::BlockHelper;

serializer!(CarnotStateCsvSerializer);

pub(crate) mod serde_block {
    use carnot_engine::LeaderProof;

    use super::{qc::QcHelper, *};

    #[derive(Serialize)]
    #[serde(untagged)]
    enum LeaderProofHelper<'a> {
        LeaderId { leader_id: NodeIdHelper<'a> },
    }

    impl<'a> From<&'a LeaderProof> for LeaderProofHelper<'a> {
        fn from(value: &'a LeaderProof) -> Self {
            match value {
                LeaderProof::LeaderId { leader_id } => Self::LeaderId {
                    leader_id: leader_id.into(),
                },
            }
        }
    }

    pub(super) struct BlockHelper<'a>(BlockHelperInner<'a>);

    #[derive(Serialize)]
    struct BlockHelperInner<'a> {
        view: View,
        id: BlockIdHelper<'a>,
        parent_qc: QcHelper<'a>,
        leader_proof: LeaderProofHelper<'a>,
    }

    impl<'a> From<&'a Block> for BlockHelper<'a> {
        fn from(val: &'a Block) -> Self {
            Self(BlockHelperInner {
                view: val.view,
                id: (&val.id).into(),
                parent_qc: (&val.parent_qc).into(),
                leader_proof: (&val.leader_proof).into(),
            })
        }
    }

    impl<'a> serde::Serialize for BlockHelper<'a> {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            serde_json::to_string(&self.0)
                .map_err(<S::Error as serde::ser::Error>::custom)
                .and_then(|s| serializer.serialize_str(s.as_str()))
        }
    }
}

pub(super) struct LocalHighQcHelper<'a>(StandardQcHelper<'a>);

impl<'a> Serialize for LocalHighQcHelper<'a> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        serde_json::to_string(&self.0)
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|s| serializer.serialize_str(s.as_str()))
    }
}

impl<'a> From<&'a StandardQc> for LocalHighQcHelper<'a> {
    fn from(value: &'a StandardQc) -> Self {
        Self(From::from(value))
    }
}

struct SafeBlocksHelper<'a>(&'a HashMap<HeaderId, Block>);

impl<'a> From<&'a HashMap<HeaderId, Block>> for SafeBlocksHelper<'a> {
    fn from(val: &'a HashMap<HeaderId, Block>) -> Self {
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
            .map(|b| serde_json::to_string(&NodeIdHelper::from(b)))
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

struct CommittedBlockHelper<'a>(&'a [HeaderId]);

impl<'a> From<&'a [HeaderId]> for CommittedBlockHelper<'a> {
    fn from(val: &'a [HeaderId]) -> Self {
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
            .map(|b| serde_json::to_string(&BlockIdHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}
