// std
use std::hash::Hash;
// crates
use serde::{Deserialize, Serialize};
// internal
use nomos_core::block::Block;

#[derive(Serialize, Deserialize)]
pub enum NetworkMessage<Tx, Blob>
where
    Tx: Clone + Eq + Hash,
    Blob: Clone + Eq + Hash,
{
    Block(Block<Tx, Blob>),
}
