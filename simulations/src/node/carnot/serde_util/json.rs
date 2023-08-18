use super::*;

serializer!(CarnotStateJsonSerializer);

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
        let iter = self.0.values();
        let mut s = serializer.serialize_seq(Some(iter.size_hint().0))?;
        for b in iter {
            s.serialize_element(&BlockHelper::from(b))?;
        }
        s.end()
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
        let iter = self.0.iter();
        let mut s = serializer.serialize_seq(Some(iter.size_hint().0))?;
        for id in iter {
            s.serialize_element(&NodeIdHelper::from(id))?;
        }
        s.end()
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
        let mut s = serializer.serialize_seq(Some(self.0.len()))?;
        for c in self.0 {
            s.serialize_element(&CommitteeHelper::from(c))?;
        }
        s.end()
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
        let mut s = serializer.serialize_seq(Some(self.0.len()))?;
        for c in self.0 {
            s.serialize_element(&BlockIdHelper::from(c))?;
        }
        s.end()
    }
}
