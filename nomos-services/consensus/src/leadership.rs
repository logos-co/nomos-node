// std
use std::marker::PhantomData;
// crates
// internal
use nomos_core::crypto::PrivateKey;
use nomos_mempool::MempoolMsg;

use super::*;

// TODO: take care of sensitve material
struct Enclave {
    key: PrivateKey,
}

pub struct Leadership<Tx, Id> {
    key: Enclave,
    mempool: OutboundRelay<MempoolMsg<Tx, Id>>,
}

pub enum LeadershipResult<'view> {
    Leader {
        block: Block,
        _view: PhantomData<&'view u8>,
    },
    NotLeader {
        _view: PhantomData<&'view u8>,
    },
}

impl<Tx, Id> Leadership<Tx, Id> {
    pub fn new(key: PrivateKey, mempool: OutboundRelay<MempoolMsg<Tx, Id>>) -> Self {
        Self {
            key: Enclave { key },
            mempool,
        }
    }

    #[allow(unused, clippy::diverging_sub_expression)]
    pub async fn try_propose_block<'view>(
        &self,
        view: &'view View,
        tip: &Tip,
        qc: Approval,
    ) -> LeadershipResult<'view> {
        let ancestor_hint = todo!("get the ancestor from the tip");
        if view.is_leader(self.key.key) {
            let (tx, rx) = tokio::sync::oneshot::channel();
            self.mempool.send(MempoolMsg::View {
                ancestor_hint,
                reply_channel: tx,
            });
            let _iter = rx.await;

            LeadershipResult::Leader {
                _view: PhantomData,
                block: todo!("form a block from the returned iterator"),
            }
        } else {
            LeadershipResult::NotLeader { _view: PhantomData }
        }
    }
}
