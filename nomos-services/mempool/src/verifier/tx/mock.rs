use nomos_core::tx::mock::MockTxVerifier;
use nomos_core::tx::Transaction;

use crate::verifier::Verifier;

impl<T: Transaction> Verifier<T> for MockTxVerifier {
    type Settings = ();

    fn new(_: Self::Settings) -> Self {
        Default::default()
    }

    fn verify(&self, item: &T) -> bool {
        self.verify_tx(item)
    }
}
