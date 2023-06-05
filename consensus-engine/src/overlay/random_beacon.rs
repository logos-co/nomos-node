use crate::types::*;
use bls_signatures::{PrivateKey, PublicKey, Serialize, Signature};
use integer_encoding::VarInt;
use rand::{seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize as SerdeSerialize};
use sha2::{Digest, Sha256};
use std::ops::Deref;

use super::LeaderSelection;

pub type Entropy = [u8];
pub type Context = [u8];

#[cfg_attr(feature = "serde", derive(SerdeSerialize, Deserialize))]
#[derive(Debug, Clone, PartialEq, Copy)]
pub struct RandomBeaconState {
    // A BLS signature, proof of correct entropy generation
    #[serde(with = "serialize_signature")]
    sig: Signature,
}

impl RandomBeaconState {
    pub fn entropy(&self) -> Box<Entropy> {
        self.sig.as_bytes().into_boxed_slice()
    }

    pub fn generate(view: View, sk: &PrivateKey) -> Self {
        let sig = sk.sign(view_to_bytes(view));
        Self { sig }
    }
}

fn happy_path_verify(state: &RandomBeaconState, context: &Context, pk: PublicKey) -> bool {
    pk.verify(state.sig, context)
}

fn sad_path_verify(
    new: &RandomBeaconState,
    prev_state: &RandomBeaconState,
    context: &Context,
) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(prev_state.entropy());
    hasher.update(context);

    let expected = hasher.finalize();
    expected.as_slice() == new.entropy().deref()
}

fn view_to_bytes(view: View) -> Box<[u8]> {
    View::encode_var_vec(view).into_boxed_slice()
}

// FIXME: the spec should be clearer on what is the expected behavior,
// for now, just use something that works
fn choice(state: &RandomBeaconState, nodes: &[NodeId]) -> NodeId {
    let mut seed = [0; 32];
    seed.copy_from_slice(&state.entropy().deref()[..32]);
    let mut rng = rand_chacha::ChaChaRng::from_seed(seed);
    *nodes.choose(&mut rng).unwrap()
}

impl LeaderSelection for RandomBeaconState {
    type Advance = (RandomBeaconState, Qc, PublicKey);

    fn next_leader(&self, nodes: &[NodeId]) -> NodeId {
        choice(self, nodes)
    }

    fn advance(&self, (rb, qc, pk): Self::Advance) -> Self {
        let context = view_to_bytes(qc.view());
        match qc {
            Qc::Aggregated(_) => {
                assert!(happy_path_verify(&rb, &context, pk));
            }

            Qc::Standard(_) => {
                assert!(sad_path_verify(&rb, self, &context));
            }
        }

        todo!()
    }
}

mod serialize_signature {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn deserialize<'de, D>(deserializer: D) -> Result<Signature, D::Error>
    where
        D: Deserializer<'de>,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        Signature::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }

    pub fn serialize<S>(sig: &Signature, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let bytes = sig.as_bytes();
        bytes.serialize(serializer)
    }
}
