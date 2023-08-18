use super::*;

serializer!(CarnotStateCsvSerializer);

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
            .map(|b| serde_json::to_string(&BlockIdHelper::from(b)))
            .collect::<Result<Vec<_>, _>>()
            .map_err(<S::Error as serde::ser::Error>::custom)
            .and_then(|val| serializer.serialize_str(&format!("[{}]", val.join(","))))
    }
}
