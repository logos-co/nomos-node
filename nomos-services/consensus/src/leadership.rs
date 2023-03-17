// std
use std::marker::PhantomData;
// crates
// internal
use nomos_core::tx::Transaction;
use nomos_core::{block::BlockHeader, crypto::PrivateKey};
use nomos_mempool::MempoolMsg;

use super::*;

// TODO: take care of sensitve material
struct Enclave {
    key: PrivateKey,
}

pub struct Leadership<Tx: Transaction> {
    key: Enclave,
    mempool: OutboundRelay<MempoolMsg<Tx>>,
}

pub enum LeadershipResult<'view, TxId: Clone + Eq + core::hash::Hash> {
    Leader {
        block: Block<TxId>,
        _view: PhantomData<&'view u8>,
    },
    NotLeader {
        _view: PhantomData<&'view u8>,
    },
}

impl<Tx> Leadership<Tx>
where
    Tx: Transaction,
    Tx::Hash: Debug,
{
    pub fn new(key: PrivateKey, mempool: OutboundRelay<MempoolMsg<Tx>>) -> Self {
        Self {
            key: Enclave { key },
            mempool,
        }
    }

    #[allow(unused, clippy::diverging_sub_expression)]
    pub async fn try_propose_block<'view, Qc>(
        &self,
        view: &'view View,
        tip: &Tip,
        qc: Qc,
    ) -> LeadershipResult<'view, Tx::Hash> {
        // TODO: get the correct ancestor for the tip
        // let ancestor_hint = todo!("get the ancestor from the tip");
        let ancestor_hint = [0; 32];
        if view.is_leader(self.key.key) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.mempool.send(MempoolMsg::View {
                ancestor_hint,
                reply_channel: tx,
            }).await;
            let iter = rx.await.unwrap();

            LeadershipResult::Leader {
                _view: PhantomData,
                block: Block::new(
                    BlockHeader::default(),
                    iter.map(|ref tx| <Tx as Transaction>::hash(tx)),
                ),
            }
        } else {
            LeadershipResult::NotLeader { _view: PhantomData }
        }
    }
}
