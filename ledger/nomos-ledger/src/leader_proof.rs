use nomos_proof_statements::leadership::LeaderPublic;

pub trait LeaderProof {
    fn verify(&self, public_inputs: &LeaderPublic) -> bool;
    // The nullifier of the coin used in the proof
    fn nullifier(&self) -> cl::Nullifier;
    // The evolved commitment for the coin used in the proof
    fn evolved_commitment(&self) -> cl::NoteCommitment;
    // The merkle root used for the proof
    // This is needed here because the proof could be using an old merkle root, and we don't want a verifying
    // node to have to guess which one it's using.
    fn merke_root(&self) -> [u8; 32];

    fn to_orphan_proof(&self) -> OrphanProof {
        OrphanProof {
            nullifier: self.nullifier(),
            commitment: self.evolved_commitment(),
            cm_root: self.merke_root(),
        }
    }
}

pub struct OrphanProof {
    pub(super) nullifier: cl::Nullifier,
    pub(super) commitment: cl::NoteCommitment,
    pub(super) cm_root: [u8; 32],
}
