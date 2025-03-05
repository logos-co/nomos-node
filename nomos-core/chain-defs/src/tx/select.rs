use std::marker::PhantomData;

use crate::{
    tx::{Transaction, TxSelect},
    utils,
};

#[derive(Default, Clone, Copy)]
pub struct FillSize<const SIZE: usize, Tx> {
    _tx: PhantomData<Tx>,
}

impl<const SIZE: usize, Tx> FillSize<SIZE, Tx> {
    #[must_use]
    pub const fn new_empty() -> Self {
        Self { _tx: PhantomData }
    }
}

impl<const SIZE: usize, Tx: Transaction> TxSelect for FillSize<SIZE, Tx> {
    type Tx = Tx;
    type Settings = ();

    fn new((): Self::Settings) -> Self {
        Self::new_empty()
    }

    fn select_tx_from<'i, I: Iterator<Item = Self::Tx> + 'i>(
        &self,
        txs: I,
    ) -> impl Iterator<Item = Self::Tx> + 'i {
        utils::select::select_from_till_fill_size::<SIZE, Self::Tx, _, _>(
            |tx| tx.as_bytes().len(),
            txs,
        )
    }
}
