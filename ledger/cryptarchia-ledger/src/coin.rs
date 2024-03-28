use crate::{crypto::Blake2b, Commitment, EpochState, LeaderProof, Nonce, Nullifier};
use blake2::digest::Digest;
use cryptarchia_engine::config::Config;
use cryptarchia_engine::Slot;

type SecretKey = [u8; 32];
type PublicKey = [u8; 32];

#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Clone, Debug, Eq, PartialEq, Copy, Hash)]
pub struct Value(u32);

impl From<u32> for Value {
    fn from(value: u32) -> Self {
        Self(value)
    }
}

// This implementatio is only a stub
// see https://github.com/logos-co/nomos-specs/blob/master/cryptarchia/cryptarchia.py for a spec
#[cfg_attr(feature = "serde", derive(serde::Serialize, serde::Deserialize))]
#[derive(Debug, Clone, PartialEq, Eq, Copy)]
pub struct Coin {
    sk: SecretKey,
    nonce: Nonce,
    value: Value,
}

impl Coin {
    pub fn new(sk: SecretKey, nonce: Nonce, value: Value) -> Self {
        Self { sk, nonce, value }
    }

    pub fn value(&self) -> Value {
        self.value
    }

    pub fn evolve(&self) -> Self {
        let mut h = Blake2b::new();
        h.update("coin-evolve");
        h.update(self.sk);
        h.update(self.nonce);
        Self {
            sk: self.sk,
            nonce: <[u8; 32]>::from(h.finalize()).into(),
            value: self.value,
        }
    }

    fn value_bytes(&self) -> [u8; 32] {
        let mut value_bytes = [0; 32];
        value_bytes[28..].copy_from_slice(&self.value.0.to_be_bytes());
        value_bytes
    }

    pub fn pk(&self) -> &PublicKey {
        &self.sk
    }

    pub fn commitment(&self) -> Commitment {
        let mut h = Blake2b::new();
        h.update("coin-commitment");
        h.update(self.nonce);
        h.update(self.pk());
        h.update(self.value_bytes());
        <[u8; 32]>::from(h.finalize()).into()
    }

    pub fn nullifier(&self) -> Nullifier {
        let mut h = Blake2b::new();
        h.update("coin-nullifier");
        h.update(self.nonce);
        h.update(self.pk());
        h.update(self.value_bytes());
        <[u8; 32]>::from(h.finalize()).into()
    }

    // TODO: better precision
    fn phi(active_slot_coeff: f64, relative_stake: f64) -> f64 {
        1.0 - (1.0 - active_slot_coeff).powf(relative_stake)
    }

    pub fn vrf(&self, epoch_nonce: Nonce, slot: Slot) -> [u8; 32] {
        let mut h = Blake2b::new();
        h.update("lead");
        h.update(epoch_nonce);
        h.update(u64::from(slot).to_be_bytes());
        h.update(self.sk);
        h.update(self.nonce);
        h.finalize().into()
    }

    pub fn is_slot_leader(&self, epoch: &EpochState, slot: Slot, config: &Config) -> bool {
        // TODO: check slot and epoch state are consistent
        let relative_stake = f64::from(self.value.0) / f64::from(epoch.total_stake.0);

        let vrf = self.vrf(epoch.nonce, slot);

        // HACK: we artificially restrict the VRF output so that it fits into a u32
        // this is not going to be the final implementation anyway and the last 4 bytes
        // should have a similar (statistical) distribution as the full output
        let vrf = u32::from_be_bytes(vrf[28..].try_into().unwrap());
        vrf < (f64::from(u32::MAX) * Self::phi(config.active_slot_coeff, relative_stake)) as u32
    }

    pub fn to_proof(&self, slot: Slot) -> LeaderProof {
        LeaderProof::new(
            self.commitment(),
            self.nullifier(),
            slot,
            self.evolve().commitment(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn test_slot_leader_statistics() {
        let epoch = EpochState {
            epoch: 0.into(),
            nonce: [1; 32].into(),
            commitments: Default::default(),
            total_stake: 1000.into(),
        };

        let coin = Coin {
            sk: [0; 32],
            nonce: [0; 32].into(),
            value: 10.into(),
        };

        let config = Config {
            active_slot_coeff: 0.05,
            security_param: 1,
        };

        // We'll use the Margin of Error equation to decide how many samples we need.
        //  https://en.wikipedia.org/wiki/Margin_of_error
        let margin_of_error = 1e-4;
        let p = Coin::phi(config.active_slot_coeff, 10.0 / 1000.0);
        let std = (p * (1.0 - p)).sqrt();
        let z = 3.0; // we want 3 std from the mean to be within the margin of error
        let n = (z * std / margin_of_error).powi(2).ceil();

        // After N slots, the measured leader rate should be within the
        // interval `p +- margin_of_error` with high probabiltiy
        let leader_rate = (0..n as u64)
            .map(|slot| coin.is_slot_leader(&epoch, slot.into(), &config))
            .filter(|x| *x)
            .count();

        println!("leader_rate: {n} {} / {}", leader_rate, p);

        assert!(
            (leader_rate as f64 / n - p).abs() < margin_of_error,
            "{leader_rate} != {p}, err={} > {margin_of_error}",
            (leader_rate as f64 / n - p).abs()
        );
    }
}
