use cl::{merkle::PathNode, note::NoteCommitment};
// up to 2^14 commitments
const MAX_NOTE_COMMS: usize = 1 << 14;

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct NoteTree {
    commitments: rpds::VectorSync<NoteCommitment>,
    // current root + previous roots
    roots: rpds::HashTrieSetSync<[u8; 32]>,
}

fn note_commitment_leaves(
    commitments: &rpds::VectorSync<NoteCommitment>,
) -> [[u8; 32]; MAX_NOTE_COMMS] {
    let note_comm_bytes = Vec::from_iter(commitments.iter().map(|c| c.as_bytes().to_vec()));
    cl::merkle::padded_leaves::<MAX_NOTE_COMMS>(&note_comm_bytes)
}

impl NoteTree {
    pub fn insert(&self, note: NoteCommitment) -> Self {
        let commitments = self.commitments.push_back(note);
        let root = cl::merkle::root(note_commitment_leaves(&commitments));
        let roots = self.roots.insert(root);
        Self { commitments, roots }
    }

    // TODO: cache if called frequently
    pub fn root(&self) -> [u8; 32] {
        cl::merkle::root(note_commitment_leaves(&self.commitments))
    }

    pub fn is_valid_root(&self, root: &[u8; 32]) -> bool {
        self.roots.contains(root)
    }

    pub fn witness(&self, index: usize) -> Option<Vec<PathNode>> {
        if index >= self.commitments.len() {
            return None;
        }
        let leaves = note_commitment_leaves(&self.commitments);
        Some(cl::merkle::path(leaves, index))
    }

    pub fn commitments(&self) -> &rpds::VectorSync<NoteCommitment> {
        &self.commitments
    }
}

impl FromIterator<NoteCommitment> for NoteTree {
    fn from_iter<I: IntoIterator<Item = NoteCommitment>>(iter: I) -> Self {
        let commitments = rpds::VectorSync::from_iter(iter);
        let root = cl::merkle::root(note_commitment_leaves(&commitments));
        Self {
            commitments,
            roots: rpds::HashTrieSetSync::default().insert(root),
        }
    }
}
