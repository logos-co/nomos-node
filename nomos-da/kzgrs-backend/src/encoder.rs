use crate::common::{Chunk, ChunksMatrix};
use kzgrs::{Commitment, Proof, BYTES_PER_FIELD_ELEMENT};

pub struct DaEncoderParams {
    column_count: usize,
    bytes_per_chunk: usize,
}

impl DaEncoderParams {
    const MAX_BLS12_381_ENCODING_CHUNK_SIZE: usize = 31;

    const fn default_with(column_count: usize) -> Self {
        Self {
            column_count,
            bytes_per_chunk: Self::MAX_BLS12_381_ENCODING_CHUNK_SIZE,
        }
    }
}

pub struct EncodedData {
    data: Vec<u8>,
    chunked_data: ChunksMatrix,
    extended_data: ChunksMatrix,
    row_commitments: Vec<Commitment>,
    row_proofs: Vec<Vec<Proof>>,
    column_commitments: Vec<Commitment>,
    aggregated_column_commitment: Commitment,
    aggregated_column_proofs: Vec<Proof>,
}

struct DaEncoder {
    params: DaEncoderParams,
}

impl DaEncoder {
    pub const fn new(settings: DaEncoderParams) -> Self {
        assert!(settings.bytes_per_chunk < BYTES_PER_FIELD_ELEMENT);
        Self { params: settings }
    }

    fn chunkify(&self, data: &[u8]) -> ChunksMatrix {
        let size = self.params.column_count * self.params.bytes_per_chunk;
        data.windows(size)
            .map(|d| {
                d.windows(self.params.bytes_per_chunk)
                    .map(Chunk::from)
                    .collect()
            })
            .collect()
    }
}
