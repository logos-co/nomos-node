use cl::{PartialTx, PartialTxWitness};
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtxPublic {
    pub ptx: PartialTx,
    pub cm_root: [u8; 32],
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct PtxPrivate {
    pub ptx: PartialTxWitness,
    pub cm_root: [u8; 32],
}
