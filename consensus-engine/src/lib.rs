use std::collections::{HashMap, HashSet};

mod types;
pub use types::*;

#[derive(Clone, Debug)]
pub struct Carnot<O: Overlay> {
    id: NodeId,
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc,
    safe_blocks: HashMap<BlockId, Block>,
    last_view_timeout_qc: Option<TimeoutQc>,
    overlay: O,
}

impl<O: Overlay> Carnot<O> {
    pub fn from_genesis(id: NodeId, genesis_block: Block, overlay: O) -> Self {
        Self {
            current_view: 0,
            local_high_qc: StandardQc::genesis(),
            id,
            highest_voted_view: -1,
            last_view_timeout_qc: None,
            overlay,
            safe_blocks: [(id, genesis_block)].into(),
        }
    }

    pub fn current_view(&self) -> View {
        self.current_view
    }
    /// Upon reception of a block
    ///
    /// Preconditions:
    ///  *  The parent-children relation between blocks must be preserved when calling
    ///     this function. In other words, you must call `receive_block(b.parent())` with
    ///     success before `receive_block(b)`.
    ///  *  Overlay changes for views < block.view should be made available before trying to process
    ///     a block by calling `receive_timeout_qc`.
    #[allow(clippy::result_unit_err)]
    pub fn receive_block(&self, block: Block) -> Result<Self, ()> {
        assert!(
            self.safe_blocks.contains_key(&block.parent()),
            "out of order view not supported, missing parent block for {block:?}",
        );

        // if the block has already been processed, return early
        if self.safe_blocks.contains_key(&block.id) {
            return Ok(self.clone());
        }

        if self.blocks_in_view(block.view).contains(&block)
            || block.view <= self.latest_committed_view()
        {
            //  TODO: Report malicious leader
            //  TODO: it could be possible that a malicious leader send a block to a node and another one to
            //  the rest of the network. The node should be able to catch up with the rest of the network after having
            //  validated that the history of the block is correct and diverged from its fork.
            //  By rejecting any other blocks except the first one received for a view this code does NOT do that.
            return Err(());
        }

        let mut new_state = self.clone();

        if new_state.block_is_safe(block.clone()) {
            new_state.safe_blocks.insert(block.id, block.clone());
            new_state.update_high_qc(block.parent_qc);
        } else {
            // Non safe block, not necessarily an error
            return Err(());
        }

        Ok(new_state)
    }

    /// Upon reception of a global timeout event
    ///
    /// Preconditions:
    pub fn receive_timeout_qc(&self, timeout_qc: TimeoutQc) -> Self {
        let mut new_state = self.clone();

        if timeout_qc.view < new_state.current_view {
            return new_state;
        }
        new_state.update_high_qc(timeout_qc.high_qc.clone());
        new_state.update_timeout_qc(timeout_qc.clone());

        new_state.current_view = timeout_qc.view + 1;
        new_state.overlay.rebuild(timeout_qc);

        new_state
    }

    /// Upon reception of a supermajority of votes for a safe block from children
    /// of the current node. It signals approval of the block to the network.
    ///
    /// Preconditions:
    /// *  `receive_block(b)` must have been called successfully before trying to approve a block b.
    /// *   A node should not attempt to vote for a block in a view earlier than the latest one it actively participated in.
    pub fn approve_block(&self, block: Block) -> (Self, Send) {
        assert!(self.safe_blocks.contains_key(&block.id));
        assert!(
            self.highest_voted_view < block.view,
            "can't vote for a block in the past"
        );

        let mut new_state = self.clone();

        new_state.highest_voted_view = block.view;

        let to = if new_state.overlay.is_member_of_root_committee(new_state.id) {
            [new_state.overlay.leader(block.view + 1)]
                .into_iter()
                .collect()
        } else {
            new_state.overlay.parent_committee(self.id)
        };
        (
            new_state,
            Send {
                to,
                payload: Payload::Vote(Vote {
                    block: block.id,
                    view: block.view,
                }),
            },
        )
    }

    /// Upon reception of a supermajority of votes for a new view from children of the current node.
    /// It signals approval of the new view to the network.
    ///
    /// Preconditions:
    /// *  `receive_timeout_qc(timeout_qc)` must have been called successfully before trying to approve a new view with that
    ///     timeout qc.
    /// *   A node should not attempt to approve a view earlier than the latest one it actively participated in.
    pub fn approve_new_view(
        &self,
        timeout_qc: TimeoutQc,
        new_views: HashSet<NewView>,
    ) -> (Self, Send) {
        let new_view = timeout_qc.view + 1;
        assert!(
            new_view
                >= self
                    .last_view_timeout_qc
                    .as_ref()
                    .map(|qc| qc.view)
                    .unwrap_or(0)
        );
        assert_eq!(
            new_views.len(),
            self.overlay.super_majority_threshold(self.id)
        );
        assert!(new_views.iter().all(|nv| self
            .overlay
            .is_member_of_child_committee(self.id, nv.sender)));
        assert!(self.highest_voted_view < new_view);
        assert!(new_views.iter().all(|nv| nv.view == new_view));
        assert!(new_views.iter().all(|nv| nv.timeout_qc == timeout_qc));

        let mut new_state = self.clone();

        let high_qc = new_views
            .iter()
            .map(|nv| &nv.high_qc)
            .chain(std::iter::once(&timeout_qc.high_qc))
            .max_by_key(|qc| qc.view())
            .unwrap();
        new_state.update_high_qc(high_qc.clone());

        let new_view_msg = NewView {
            view: new_view,
            high_qc: high_qc.clone(),
            sender: new_state.id,
            timeout_qc,
        };

        new_state.highest_voted_view = new_view;
        let to = if new_state.overlay.is_member_of_root_committee(new_state.id) {
            [new_state.overlay.leader(new_view + 1)]
                .into_iter()
                .collect()
        } else {
            new_state.overlay.parent_committee(new_state.id)
        };
        (
            new_state,
            Send {
                to,
                payload: Payload::NewView(new_view_msg),
            },
        )
    }

    /// Upon a configurable amout of time has elapsed since the last view change
    ///
    /// Preconditions: none!
    /// Just notice that the timer only reset after a view change, i.e. a node can't timeout
    /// more than once for the same view
    pub fn local_timeout(&self) -> (Self, Option<Send>) {
        let mut new_state = self.clone();

        new_state.highest_voted_view = new_state.current_view;
        if new_state.overlay.is_member_of_root_committee(new_state.id)
            || new_state.overlay.is_child_of_root_committee(new_state.id)
        {
            let timeout_msg = Timeout {
                view: new_state.current_view,
                high_qc: Qc::Standard(new_state.local_high_qc.clone()),
                sender: new_state.id,
                timeout_qc: new_state.last_view_timeout_qc.clone(),
            };
            let to = new_state.overlay.root_committee();
            return (
                new_state,
                Some(Send {
                    to,
                    payload: Payload::Timeout(timeout_msg),
                }),
            );
        }
        (new_state, None)
    }

    fn block_is_safe(&self, block: Block) -> bool {
        block.view >= self.current_view && block.view == block.parent_qc.view() + 1
    }

    fn update_high_qc(&mut self, qc: Qc) {
        let qc_view = qc.view();
        match qc {
            Qc::Standard(new_qc) if new_qc.view > self.local_high_qc.view => {
                self.local_high_qc = new_qc;
            }
            Qc::Aggregated(new_qc) if new_qc.high_qc.view != self.local_high_qc.view => {
                self.local_high_qc = new_qc.high_qc;
            }
            _ => {}
        }
        if qc_view == self.current_view {
            self.current_view += 1;
        }
    }

    fn update_timeout_qc(&mut self, timeout_qc: TimeoutQc) {
        match (&self.last_view_timeout_qc, timeout_qc) {
            (None, timeout_qc) => {
                self.last_view_timeout_qc = Some(timeout_qc);
            }
            (Some(current_qc), timeout_qc) if timeout_qc.view > current_qc.view => {
                self.last_view_timeout_qc = Some(timeout_qc);
            }
            _ => {}
        }
    }

    pub fn blocks_in_view(&self, view: View) -> Vec<Block> {
        self.safe_blocks
            .iter()
            .filter(|(_, b)| b.view == view)
            .map(|(_, block)| block.clone())
            .collect()
    }

    pub fn genesis_block(&self) -> Block {
        self.blocks_in_view(0)[0].clone()
    }

    // Returns the id of the grandparent block if it can be committed or None otherwise
    fn can_commit_grandparent(&self, block: Block) -> Option<Block> {
        let parent = self.safe_blocks.get(&block.parent())?;
        let grandparent = self.safe_blocks.get(&parent.parent())?;

        if parent.view == grandparent.view + 1
            && matches!(parent.parent_qc, Qc::Standard { .. })
            && matches!(grandparent.parent_qc, Qc::Standard { .. })
        {
            return Some(grandparent.clone());
        }
        None
    }

    pub fn latest_committed_block(&self) -> Block {
        for view in (0..self.current_view).rev() {
            for block in self.blocks_in_view(view) {
                if let Some(block) = self.can_commit_grandparent(block) {
                    return block;
                }
            }
        }
        self.genesis_block()
    }

    pub fn latest_committed_view(&self) -> View {
        self.latest_committed_block().view
    }

    pub fn committed_blocks(&self) -> Vec<BlockId> {
        let mut res = vec![];
        let mut current = self.latest_committed_block();
        while current != self.genesis_block() {
            res.push(current.id);
            current = self.safe_blocks.get(&current.parent()).unwrap().clone();
        }
        // If the length is 1, it means that the genesis block is the only committed block
        // and was added to the list already at the beginning of the function.
        // Otherwise, we need to add the genesis block to the list.
        if res.len() > 1 {
            res.push(self.genesis_block().id);
        }
        res
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[derive(Clone)]
    struct NoOverlay;

    impl Overlay for NoOverlay {
        fn root_committee(&self) -> Committee {
            todo!()
        }

        fn rebuild(&mut self, _timeout_qc: TimeoutQc) {
            todo!()
        }

        fn is_member_of_child_committee(&self, _parent: NodeId, _child: NodeId) -> bool {
            todo!()
        }

        fn is_member_of_root_committee(&self, _id: NodeId) -> bool {
            todo!()
        }

        fn is_member_of_leaf_committee(&self, _id: NodeId) -> bool {
            todo!()
        }

        fn is_child_of_root_committee(&self, _id: NodeId) -> bool {
            todo!()
        }

        fn parent_committee(&self, _id: NodeId) -> Committee {
            todo!()
        }

        fn leaf_committees(&self, _id: NodeId) -> HashSet<Committee> {
            todo!()
        }

        fn leader(&self, _view: View) -> NodeId {
            todo!()
        }

        fn super_majority_threshold(&self, _id: NodeId) -> usize {
            todo!()
        }

        fn leader_super_majority_threshold(&self, _view: View) -> usize {
            todo!()
        }
    }

    #[test]
    fn block_is_committed() {
        let genesis = Block {
            view: 0,
            id: [0; 32],
            parent_qc: Qc::Standard(StandardQc {
                view: 0,
                id: [0; 32],
            }),
        };
        let mut engine = Carnot::from_genesis([0; 32], genesis.clone(), NoOverlay);
        let p1 = Block {
            view: 1,
            id: [1; 32],
            parent_qc: Qc::Standard(StandardQc {
                view: 0,
                id: [0; 32],
            }),
        };
        let p2 = Block {
            view: 2,
            id: [2; 32],
            parent_qc: Qc::Standard(StandardQc {
                view: 1,
                id: [1; 32],
            }),
        };
        assert_eq!(engine.latest_committed_block(), genesis);
        engine = engine.receive_block(p1).unwrap();
        engine = engine.receive_block(p2).unwrap();
        assert_eq!(engine.latest_committed_block(), genesis);
    }
}
