use crate::account::AccountId;
use crate::crypto::Signature;

/// Verified transactions
///
/// Can only be constructed if the signature is valid,
/// but does not imply that it can be successfully applied
/// to the ledger.
#[derive(Clone, Debug)]
pub struct Transaction {
    pub from: AccountId,
    pub to: AccountId,
    pub value: u64,
    // TODO: here for the moment because I still want to retain the ability
    // to go from `Transaction` to wire format. We could otherwise
    // save the id and rely on some storage
    _signature: Signature,
}

mod serde {
    use super::*;
    use ::serde::{Deserialize, Deserializer, Serialize, Serializer};
    // We have this additional definition so that we can automatically derive
    // Serialize/Deserialize for the type while still being able to check
    // the signature while deserializing.
    // This would also allow to control ser/de independently from the Rust
    // representation.
    #[derive(Serialize, Deserialize)]
    struct WireTransaction {
        from: AccountId,
        to: AccountId,
        value: u64,
        signature: Signature,
    }

    impl<'de> Deserialize<'de> for Transaction {
        fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
        where
            D: Deserializer<'de>,
        {
            let WireTransaction {
                from,
                to,
                value,
                signature,
            } = WireTransaction::deserialize(deserializer)?;
            //TODO: check signature
            Ok(Transaction {
                from,
                to,
                value,
                _signature: signature,
            })
        }
    }

    impl Serialize for Transaction {
        fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
        where
            S: Serializer,
        {
            WireTransaction {
                from: self.from.clone(),
                to: self.to.clone(),
                value: self.value,
                signature: self._signature,
            }
            .serialize(serializer)
        }
    }
}
