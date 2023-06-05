use crate::types::*;
use bls_signatures::{PrivateKey, PublicKey, Serialize, Signature};
use integer_encoding::VarInt;
use rand::{seq::SliceRandom, SeedableRng};
use serde::{Deserialize, Serialize as SerdeSerialize};
use sha2::{Digest, Sha256};
use std::ops::Deref;
use thiserror::Error;

use super::LeaderSelection;

pub type Entropy = [u8];
pub type Context = [u8];

#[cfg_attr(feature = "serde", derive(SerdeSerialize, Deserialize))]
#[derive(Debug, Clone, PartialEq)]
pub enum RandomBeaconState {
    Happy {
        // a byte string so that we can access the entropy without
        // copying memory (can't directly go from the signature to its compressed form)
        // We still assume the conversion does not fail, so the format has to be checked
        // during deserialization
        sig: Box<[u8]>,
        #[serde(with = "serialize_bls")]
        public_key: PublicKey,
    },
    Sad {
        entropy: Box<Entropy>,
    },
}

#[derive(Debug, Error)]
pub enum Error {
    #[error("Invalid random beacon trasition")]
    InvalidRandomBeacon,
    #[error("Invalid signature format")]
    InvalidSignature(#[from] bls_signatures::Error),
}

impl RandomBeaconState {
    pub fn entropy(&self) -> &Entropy {
        match self {
            Self::Happy { sig, .. } => sig,
            Self::Sad { entropy } => entropy,
        }
    }

    pub fn generate_happy(view: View, sk: &PrivateKey) -> Self {
        let sig = sk.sign(view_to_bytes(view));
        Self::Happy {
            sig: sig.as_bytes().into(),
            public_key: sk.public_key(),
        }
    }

    pub fn generate_sad(view: View, prev: &Self) -> Self {
        let context = view_to_bytes(view);
        let mut hasher = Sha256::new();
        hasher.update(prev.entropy());
        hasher.update(context);

        let entropy = hasher.finalize().to_vec().into();
        Self::Sad { entropy }
    }

    pub fn check_advance_happy(&self, rb: RandomBeaconState, view: View) -> Result<Self, Error> {
        let context = view_to_bytes(view);
        match rb {
            Self::Happy {
                ref sig,
                public_key,
            } => {
                let sig = Signature::from_bytes(sig).unwrap();
                if !public_key.verify(sig, context) {
                    return Err(Error::InvalidRandomBeacon);
                }
            }
            Self::Sad { .. } => return Err(Error::InvalidRandomBeacon),
        }
        Ok(rb)
    }
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
    fn next_leader(&self, nodes: &[NodeId]) -> NodeId {
        choice(self, nodes)
    }
}

mod serialize_bls {
    use super::*;
    use serde::{Deserializer, Serializer};

    pub fn deserialize<'de, D, T>(deserializer: D) -> Result<T, D::Error>
    where
        D: Deserializer<'de>,
        T: bls_signatures::Serialize,
    {
        let bytes = Vec::<u8>::deserialize(deserializer)?;
        T::from_bytes(&bytes).map_err(serde::de::Error::custom)
    }

    pub fn serialize<S, T>(sig: &T, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
        T: bls_signatures::Serialize,
    {
        let bytes = sig.as_bytes();
        bytes.serialize(serializer)
    }
}
