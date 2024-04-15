use ark_serialize::CanonicalSerialize;
use blake2::digest::{Update, VariableOutput};
use kzgrs::Commitment;
use std::io::Cursor;

#[derive(Clone)]
pub struct Chunk(Vec<u8>);
pub struct Row(Vec<Chunk>);
pub struct Column(Vec<Chunk>);
pub struct ChunksMatrix(Vec<Row>);

impl Chunk {
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.to_vec()
    }

    pub const fn empty() -> Self {
        Self(vec![])
    }
}

impl From<&[u8]> for Chunk {
    fn from(value: &[u8]) -> Self {
        Self(value.to_vec())
    }
}

impl Row {
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(Chunk::as_bytes).collect()
    }
}

impl Column {
    pub fn as_bytes(&self) -> Vec<u8> {
        self.0.iter().flat_map(Chunk::as_bytes).collect()
    }
}

impl FromIterator<Chunk> for Row {
    fn from_iter<T: IntoIterator<Item = Chunk>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl FromIterator<Chunk> for Column {
    fn from_iter<T: IntoIterator<Item = Chunk>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

impl ChunksMatrix {
    pub fn rows(&self) -> impl Iterator<Item = &Row> + '_ {
        self.0.iter()
    }
    pub fn columns(&self) -> impl Iterator<Item = Column> + '_ {
        let size = self.0.first().map(|r| r.0.len()).unwrap_or(0);
        (0..size).map(|i| {
            self.0
                .iter()
                .map(|row| row.0.get(i).cloned().unwrap_or_else(Chunk::empty))
                .collect::<Column>()
        })
    }

    pub fn transposed(&self) -> Self {
        Self(self.columns().map(|c| Row(c.0)).collect())
    }
}

impl FromIterator<Row> for ChunksMatrix {
    fn from_iter<T: IntoIterator<Item = Row>>(iter: T) -> Self {
        Self(iter.into_iter().collect())
    }
}

pub fn hash_column_and_commitment<const HASH_SIZE: usize>(
    column: &Column,
    commitment: &Commitment,
) -> [u8; HASH_SIZE] {
    use ark_serialize::CanonicalSerialize;
    let mut hasher = blake2::Blake2bVar::new(HASH_SIZE)
        .unwrap_or_else(|e| panic!("Blake2b should work for size {HASH_SIZE}, {e}"));
    hasher.update(column.as_bytes().as_ref());
    let mut buff = Cursor::new(vec![]);
    commitment
        .serialize_uncompressed(&mut buff)
        .expect("Serialization of commitment should work");
    hasher.update(buff.into_inner().as_ref());
    hasher
        .finalize_boxed()
        .to_vec()
        .try_into()
        .unwrap_or_else(|_| panic!("Size is guaranteed by constant {HASH_SIZE:?}"))
}
