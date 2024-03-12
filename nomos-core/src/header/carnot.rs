use super::{ContentId, HeaderId};
use crate::crypto::Blake2b;
use crate::wire;
use blake2::Digest;
use serde::{Deserialize, Serialize};

use carnot_engine::overlay::RandomBeaconState;
use carnot_engine::{LeaderProof, Qc, View};

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct Header {
    beacon: RandomBeaconState,
    view: View,
    parent_qc: Qc<HeaderId>,
    leader_proof: LeaderProof,
    content_id: ContentId,
    content_size: u32,
}

impl Header {
    pub fn new(
        beacon: RandomBeaconState,
        view: View,
        parent_qc: Qc<HeaderId>,
        leader_proof: LeaderProof,
        content_id: ContentId,
        content_size: u32,
    ) -> Self {
        Self {
            beacon,
            view,
            parent_qc,
            leader_proof,
            content_id,
            content_size,
        }
    }

    pub fn beacon(&self) -> &RandomBeaconState {
        &self.beacon
    }

    pub fn id(&self) -> HeaderId {
        let mut h = Blake2b::new();
        let bytes = wire::serialize(&self).unwrap();
        h.update(&bytes);
        HeaderId(h.finalize().into())
    }

    pub fn parent_qc(&self) -> &Qc<HeaderId> {
        &self.parent_qc
    }

    pub fn leader_proof(&self) -> &LeaderProof {
        &self.leader_proof
    }

    pub fn content_id(&self) -> ContentId {
        self.content_id
    }

    pub fn content_size(&self) -> u32 {
        self.content_size
    }

    pub fn view(&self) -> View {
        self.view
    }

    pub fn parent(&self) -> HeaderId {
        self.parent_qc.block()
    }

    pub fn to_carnot_block(&self) -> carnot_engine::Block<HeaderId> {
        carnot_engine::Block {
            id: self.id(),
            parent_qc: self.parent_qc.clone(),
            view: self.view(),
            leader_proof: self.leader_proof().clone(),
        }
    }
}

pub struct Builder {
    beacon: RandomBeaconState,
    view: View,
    parent_qc: Qc<HeaderId>,
    leader_proof: LeaderProof,
}

impl Builder {
    pub fn new(
        beacon: RandomBeaconState,
        view: View,
        parent_qc: Qc<HeaderId>,
        leader_proof: LeaderProof,
    ) -> Self {
        Self {
            beacon,
            view,
            parent_qc,
            leader_proof,
        }
    }

    pub fn build(self, content_id: ContentId, content_size: u32) -> Header {
        Header::new(
            self.beacon,
            self.view,
            self.parent_qc,
            self.leader_proof,
            content_id,
            content_size,
        )
    }
}

