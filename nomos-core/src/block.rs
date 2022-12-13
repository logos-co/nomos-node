/// A block
#[derive(Clone)]
pub struct Block;

/// A block chunk, N pieces are necessary to reconstruct the full block
#[derive(Clone, Copy, Debug)]
pub struct BlockChunk {
    pub index: u8,
}

impl Block {
    /// Fake implementation of erasure coding protocol
    pub fn chunk<const SIZE: usize>(self) -> [BlockChunk; SIZE] {
        // TODO: this is a completely temporary and fake implementation
        (0..SIZE)
            .map(|i| BlockChunk { index: i as u8 })
            .collect::<Vec<_>>()
            .try_into()
            .expect("This should not fail unless chunking exceed memory limits")
    }
}
