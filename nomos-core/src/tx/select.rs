// std
use std::marker::PhantomData;
// crates

// internal
use crate::tx::{Transaction, TxSelect};
use crate::utils;

#[derive(Default, Clone, Copy)]
pub struct FillSize<const SIZE: usize, Tx> {
    _tx: PhantomData<Tx>,
}

impl<const SIZE: usize, Tx> FillSize<SIZE, Tx> {
    pub fn new() -> Self {
        Self {
            _tx: Default::default(),
        }
    }
}

impl<const SIZE: usize, Tx: Transaction> TxSelect for FillSize<SIZE, Tx> {
    type Tx = Tx;
    type Settings = ();

    fn new(_: Self::Settings) -> Self {
        FillSize::new()
    }

    fn select_tx_from<'i, I: Iterator<Item = Self::Tx> + 'i>(
        &self,
        txs: I,
    ) -> impl Iterator<Item = Self::Tx> + 'i {
        utils::select::select_from_till_fill_size::<SIZE, Self::Tx>(|tx| tx.as_bytes().len(), txs)
    }
}
