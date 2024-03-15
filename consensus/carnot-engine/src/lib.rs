use std::{
    collections::{HashMap, HashSet},
    hash::Hash,
};

pub mod overlay;
mod types;
pub use overlay::Overlay;
pub use types::*;

#[derive(Clone, Debug, PartialEq)]
pub struct Carnot<O: Overlay, Id: Eq + Hash> {
    id: NodeId,
    current_view: View,
    highest_voted_view: View,
    local_high_qc: StandardQc<Id>,
    safe_blocks: HashMap<Id, Block<Id>>,
    tip: Id,
    last_view_timeout_qc: Option<TimeoutQc<Id>>,
    latest_committed_block: Option<Id>,
    overlay: O,
}

impl<O, Id> Carnot<O, Id>
where
    O: Overlay,
    Id: Copy + Eq + Hash + core::fmt::Debug,
{
    pub fn from_genesis(id: NodeId, genesis_block: Block<Id>, overlay: O) -> Self {
        Self {
            current_view: View(0),
            local_high_qc: StandardQc::genesis(genesis_block.id),
            id,
            highest_voted_view: View(-1),
            last_view_timeout_qc: None,
            latest_committed_block: None,
            overlay,
            safe_blocks: [(genesis_block.id, genesis_block.clone())].into(),
            tip: genesis_block.id,
        }
    }

    pub fn current_view(&self) -> View {
        self.current_view
    }

    pub fn highest_voted_view(&self) -> View {
        self.highest_voted_view
    }

    pub fn safe_blocks(&self) -> &HashMap<Id, Block<Id>> {
        &self.safe_blocks
    }

    /// Return the most recent safe block
    pub fn tip(&self) -> Block<Id> {
        self.safe_blocks[&self.tip].clone()
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
    pub fn receive_block(&self, block: Block<Id>) -> Result<Self, ()> {
        assert!(
            self.safe_blocks.contains_key(&block.parent()),
            "out of order view not supported, missing parent block for {block:?}",
        );
        // if the block has already been processed, return early
        if self.safe_blocks.contains_key(&block.id) {
            return Ok(self.clone());
        }

        match block.leader_proof {
            LeaderProof::LeaderId { leader_id } => {
                // This only accepts blocks from the leader of current_view + 1
                if leader_id != self.overlay.next_leader()
                    || block.view != self.current_view().next()
                {
                    return Err(());
                }
            }
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
            if block.view > new_state.tip().view {
                new_state.tip = block.id;
                new_state.update_latest_committed_block(&block);
            }
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
    pub fn receive_timeout_qc(&self, timeout_qc: TimeoutQc<Id>) -> Self {
        let mut new_state = self.clone();

        if timeout_qc.view() < new_state.current_view {
            return new_state;
        }
        new_state.update_high_qc(Qc::Standard(timeout_qc.high_qc().clone()));
        new_state.update_timeout_qc(timeout_qc.clone());

        new_state.current_view = timeout_qc.view().next();

        new_state
    }

    /// Upon reception of a supermajority of votes for a safe block from children
    /// of the current node. It signals approval of the block to the network.
    ///
    /// Preconditions:
    /// *  `receive_block(b)` must have been called successfully before trying to approve a block b.
    /// *   A node should not attempt to vote for a block in a view earlier than the latest one it actively participated in.
    pub fn approve_block(&self, block: Block<Id>) -> (Self, Send<Id>) {
        assert!(
            self.safe_blocks.contains_key(&block.id),
            "{:?} not in {:?}",
            block,
            self.safe_blocks
        );
        assert!(
            self.highest_voted_view < block.view,
            "can't vote for a block in the past"
        );

        let mut new_state = self.clone();

        new_state.highest_voted_view = block.view;

        let to = if new_state.overlay.is_member_of_root_committee(new_state.id) {
            [new_state.overlay.next_leader()].into_iter().collect()
        } else {
            new_state
                .overlay
                .parent_committee(self.id)
                .expect("Non root committee members parent should be present")
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
        timeout_qc: TimeoutQc<Id>,
        new_views: HashSet<NewView<Id>>,
    ) -> (Self, Send<Id>) {
        let new_view = timeout_qc.view().next();
        assert!(
            new_view
                > self
                    .last_view_timeout_qc
                    .as_ref()
                    .map(|qc| qc.view())
                    .unwrap_or(View(0)),
            "can't vote for a new view not bigger than the last timeout_qc"
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
            .chain(std::iter::once(timeout_qc.high_qc()))
            .max_by_key(|qc| qc.view)
            .unwrap();
        new_state.update_high_qc(Qc::Standard(high_qc.clone()));

        let new_view_msg = NewView {
            view: new_view,
            high_qc: high_qc.clone(),
            sender: new_state.id,
            timeout_qc,
        };

        new_state.highest_voted_view = new_view;
        let to = if new_state.overlay.is_member_of_root_committee(new_state.id) {
            [new_state.overlay.next_leader()].into_iter().collect()
        } else {
            new_state
                .overlay
                .parent_committee(new_state.id)
                .expect("Non root committee members parent should be present")
        };
        (
            new_state,
            Send {
                to,
                payload: Payload::NewView(new_view_msg),
            },
        )
    }

    /// Upon a configurable amount of time has elapsed since the last view change
    ///
    /// Preconditions: none!
    /// Just notice that the timer only reset after a view change, i.e. a node can't timeout
    /// more than once for the same view
    pub fn local_timeout(&self) -> (Self, Option<Send<Id>>) {
        let mut new_state = self.clone();

        new_state.highest_voted_view = new_state.current_view;
        if new_state.overlay.is_member_of_root_committee(new_state.id)
            || new_state.overlay.is_child_of_root_committee(new_state.id)
        {
            let timeout_msg = Timeout {
                view: new_state.current_view,
                high_qc: new_state.local_high_qc.clone(),
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

    fn block_is_safe(&self, block: Block<Id>) -> bool {
        block.view >= self.current_view && block.view == block.parent_qc.view().next()
    }

    fn update_high_qc(&mut self, qc: Qc<Id>) {
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
            self.current_view += View(1);
        }
    }

    fn update_timeout_qc(&mut self, timeout_qc: TimeoutQc<Id>) {
        match (&self.last_view_timeout_qc, timeout_qc) {
            (None, timeout_qc) => {
                self.last_view_timeout_qc = Some(timeout_qc);
            }
            (Some(current_qc), timeout_qc) if timeout_qc.view() > current_qc.view() => {
                self.last_view_timeout_qc = Some(timeout_qc);
            }
            _ => {}
        }
    }

    fn update_latest_committed_block(&mut self, block: &Block<Id>) {
        if let Some(block) = self.can_commit_grandparent(block) {
            self.latest_committed_block = Some(block.id);
        }
    }

    pub fn blocks_in_view(&self, view: View) -> Vec<Block<Id>> {
        self.safe_blocks
            .iter()
            .filter(|(_, b)| b.view == view)
            .map(|(_, block)| block.clone())
            .collect()
    }

    pub fn genesis_block(&self) -> Block<Id> {
        self.blocks_in_view(View(0))[0].clone()
    }

    // Returns the id of the grandparent block if it can be committed or None otherwise
    fn can_commit_grandparent(&self, block: &Block<Id>) -> Option<Block<Id>> {
        let parent = self.safe_blocks.get(&block.parent())?;
        let grandparent = self.safe_blocks.get(&parent.parent())?;

        if parent.view == grandparent.view.next()
            && matches!(parent.parent_qc, Qc::Standard { .. })
            && matches!(grandparent.parent_qc, Qc::Standard { .. })
        {
            return Some(grandparent.clone());
        }
        None
    }

    pub fn latest_committed_block(&self) -> Block<Id> {
        self.latest_committed_block
            .and_then(|id| self.safe_blocks.get(&id).cloned())
            .unwrap_or_else(|| self.genesis_block())
    }

    pub fn latest_committed_view(&self) -> View {
        self.latest_committed_block().view
    }

    pub fn latest_committed_blocks(&self, limit: Option<usize>) -> Vec<Id> {
        let limit = limit.unwrap_or(self.safe_blocks.len());
        let mut res = vec![];
        let mut current = self.latest_committed_block();

        while res.len() < limit {
            res.push(current.id);

            if current == self.genesis_block() {
                break;
            }

            current = match self.safe_blocks.get(&current.parent()) {
                Some(new_current) => new_current.clone(),
                None => break,
            };
        }

        res
    }

    pub fn last_view_timeout_qc(&self) -> Option<TimeoutQc<Id>> {
        self.last_view_timeout_qc.clone()
    }

    pub fn high_qc(&self) -> StandardQc<Id> {
        self.local_high_qc.clone()
    }

    pub fn is_next_leader(&self) -> bool {
        self.overlay.next_leader() == self.id
    }

    pub fn super_majority_threshold(&self) -> usize {
        self.overlay.super_majority_threshold(self.id)
    }

    pub fn leader_super_majority_threshold(&self) -> usize {
        self.overlay.leader_super_majority_threshold(self.id)
    }

    pub fn id(&self) -> NodeId {
        self.id
    }

    pub fn self_committee(&self) -> Committee {
        self.overlay.node_committee(self.id)
    }

    pub fn child_committees(&self) -> Vec<Committee> {
        self.overlay.child_committees(self.id)
    }

    pub fn parent_committee(&self) -> Option<Committee> {
        self.overlay.parent_committee(self.id)
    }

    pub fn root_committee(&self) -> Committee {
        self.overlay.root_committee()
    }

    pub fn is_member_of_root_committee(&self) -> bool {
        self.overlay.is_member_of_root_committee(self.id)
    }

    pub fn overlay(&self) -> &O {
        &self.overlay
    }

    /// A way to allow for overlay extendability without compromising the engine
    /// generality.
    pub fn update_overlay<F, E>(&self, f: F) -> Result<Self, E>
    where
        F: FnOnce(O) -> Result<O, E>,
    {
        match f(self.overlay.clone()) {
            Ok(overlay) => Ok(Self {
                overlay,
                ..self.clone()
            }),
            Err(e) => Err(e),
        }
    }

    /// Blocks newer than the last committed block are not safe to be pruned
    pub fn prune_older_blocks_by_view(&mut self, threshold_view: View) {
        assert!(threshold_view < self.latest_committed_block().view);
        // do not remove genesis
        let view_zero = View::new(0);
        self.safe_blocks
            .retain(|_, b| b.view > threshold_view || view_zero == b.view);
    }
}

#[cfg(test)]
mod test {
    use std::convert::Infallible;

    use crate::overlay::{FlatOverlay, FlatOverlaySettings, FreezeMembership, RoundRobin};

    use super::*;

    fn init(nodes: Vec<NodeId>) -> Carnot<FlatOverlay<RoundRobin, FreezeMembership>, usize> {
        assert!(!nodes.is_empty());

        Carnot::from_genesis(
            *nodes.first().unwrap(),
            Block {
                view: View(0),
                id: 0,
                parent_qc: Qc::Standard(StandardQc::genesis(0)),
                leader_proof: LeaderProof::LeaderId {
                    leader_id: *nodes.first().unwrap(),
                },
            },
            FlatOverlay::new(FlatOverlaySettings {
                nodes,
                leader: RoundRobin::default(),
                leader_super_majority_threshold: None,
            }),
        )
    }

    fn next_block(
        engine: &Carnot<FlatOverlay<RoundRobin, FreezeMembership>, usize>,
        block: &Block<usize>,
    ) -> Block<usize> {
        let next_id = block.id + 1;

        Block {
            view: block.view.next(),
            id: next_id,
            parent_qc: Qc::Standard(StandardQc {
                view: block.view,
                id: block.id,
            }),
            leader_proof: LeaderProof::LeaderId {
                leader_id: engine.overlay().next_leader(),
            },
        }
    }

    fn update_leader_selection(
        engine: &Carnot<FlatOverlay<RoundRobin, FreezeMembership>, usize>,
    ) -> Carnot<FlatOverlay<RoundRobin, FreezeMembership>, usize> {
        engine
            .update_overlay(|overlay| {
                overlay.update_leader_selection(
                    |leader_selection| -> Result<RoundRobin, Infallible> {
                        Ok(leader_selection.advance())
                    },
                )
            })
            .unwrap()
    }

    #[test]
    // Ensure that all states are initialized correctly with the genesis block.
    fn from_genesis() {
        let engine = init(vec![NodeId::new([0; 32])]);
        assert_eq!(engine.current_view(), View(0));
        assert_eq!(engine.highest_voted_view, View(-1));

        let genesis = engine.genesis_block();
        assert_eq!(engine.high_qc(), genesis.parent_qc.high_qc());
        assert_eq!(engine.blocks_in_view(View(0)), vec![genesis.clone()]);
        assert_eq!(engine.last_view_timeout_qc(), None);
        assert_eq!(engine.latest_committed_blocks(None), vec![genesis.id]);
    }

    #[test]
    // Ensure that all states are updated correctly after a block is received.
    fn receive_block() {
        let mut engine = init(vec![NodeId::new([0; 32])]);
        let block = next_block(&engine, &engine.genesis_block());

        engine = engine.receive_block(block.clone()).unwrap();
        assert_eq!(engine.blocks_in_view(block.view), vec![block.clone()]);
        assert_eq!(engine.high_qc(), block.parent_qc.high_qc());
        assert_eq!(engine.latest_committed_block(), engine.genesis_block());
    }

    #[test]
    // Ensure that receive_block() returns early if the same block ID has already been received.
    fn receive_duplicate_block_id() {
        let mut engine = init(vec![NodeId::new([0; 32])]);

        let block1 = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block1.clone()).unwrap();
        engine = update_leader_selection(&engine);

        let mut block2 = next_block(&engine, &block1);
        block2.id = block1.id;
        engine = engine.receive_block(block2).unwrap();
        assert_eq!(engine.blocks_in_view(View(1)), vec![block1]);
    }

    #[test]
    #[should_panic(expected = "out of order view not supported, missing parent block")]
    // Ensure that receive_block() fails if the parent block has never been received.
    fn receive_block_with_unknown_parent() {
        let engine = init(vec![NodeId::new([0; 32])]);
        let parent_block_id = 42;
        let block = Block {
            view: engine.current_view().next(),
            id: 1,
            parent_qc: Qc::Standard(StandardQc {
                view: engine.current_view(),
                id: parent_block_id,
            }),
            leader_proof: LeaderProof::LeaderId {
                leader_id: NodeId::new([0; 32]),
            },
        };

        let _ = engine.receive_block(block);
    }

    #[test]
    // Ensure that receive_block() returns Err for unsafe blocks.
    fn receive_unsafe_blocks() {
        let mut engine = init(vec![NodeId::new([0; 32])]);

        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block.clone()).unwrap();
        engine = update_leader_selection(&engine);

        let block = next_block(&engine, &block);
        engine = engine.receive_block(block.clone()).unwrap();
        engine = update_leader_selection(&engine);

        let mut unsafe_block = next_block(&engine, &block);
        unsafe_block.view = engine.current_view().prev(); // UNSAFE: view < engine.current_view
        assert!(engine.receive_block(unsafe_block).is_err());

        let mut unsafe_block = next_block(&engine, &block);
        unsafe_block.view = unsafe_block.parent_qc.view() + View(2); // UNSAFE: view != parent_qc.view + 1
        assert!(engine.receive_block(unsafe_block).is_err());
    }

    #[test]
    // Ensure that the grandparent of the current view can be committed
    fn receive_block_and_commit() {
        let mut engine = init(vec![NodeId::new([0; 32])]);
        assert_eq!(engine.latest_committed_block(), engine.genesis_block());

        let block1 = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block1.clone()).unwrap();
        assert_eq!(engine.latest_committed_block(), engine.genesis_block());
        engine = update_leader_selection(&engine);

        let block2 = next_block(&engine, &block1);
        engine = engine.receive_block(block2.clone()).unwrap();
        assert_eq!(engine.latest_committed_block(), engine.genesis_block());
        engine = update_leader_selection(&engine);

        let block3 = next_block(&engine, &block2);
        engine = engine.receive_block(block3.clone()).unwrap();
        assert_eq!(engine.latest_committed_block(), block1);
        assert_eq!(
            engine.latest_committed_blocks(None),
            vec![block1.id, engine.genesis_block().id] // without block2 and block3
        );
        engine = update_leader_selection(&engine);

        let block4 = next_block(&engine, &block3);
        engine = engine.receive_block(block4).unwrap();
        assert_eq!(engine.latest_committed_block(), block2);
        assert_eq!(
            engine.latest_committed_blocks(None),
            vec![block2.id, block1.id, engine.genesis_block().id] // without block3, block4
        );
    }

    #[test]
    // Ensure that the leader check in receive_block fails
    // if the block is proposed by an unexpected leader.
    fn receive_block_with_unexpected_leader() {
        let mut engine = init(vec![NodeId::new([0; 32]), NodeId::new([1; 32])]);

        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block.clone()).unwrap();
        engine = update_leader_selection(&engine);
        // now next_leader is [1; 32]

        let mut block = next_block(&engine, &block);
        block.leader_proof = LeaderProof::LeaderId {
            leader_id: NodeId::new([0; 32]), // unexpected leader
        };
        assert!(engine.receive_block(block).is_err());
    }

    #[test]
    // Ensure that the leader check in receive_block fails
    // if block.view is not the expected view.
    fn receive_block_with_unexpected_view() {
        let mut engine = init(vec![NodeId::new([0; 32]), NodeId::new([1; 32])]);

        let block1 = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block1.clone()).unwrap();
        assert_eq!(engine.current_view(), View(1));
        engine = update_leader_selection(&engine);

        // a future block should be rejected
        let future_block = Block {
            id: 10,
            view: View(11), // a future view
            parent_qc: Qc::Aggregated(AggregateQc {
                view: View(10),
                high_qc: StandardQc {
                    // a known parent block
                    id: block1.id,
                    view: block1.view,
                },
            }),
            leader_proof: LeaderProof::LeaderId {
                leader_id: engine.overlay().next_leader(),
            },
        };
        assert!(engine.receive_block(future_block).is_err());

        // a past block should be also rejected
        let mut past_block = block1; // with the same view as block1
        past_block.id = 10;
        assert!(engine.receive_block(past_block).is_err());
    }

    #[test]
    // Ensure that approve_block updates highest_voted_view and returns a correct Send.
    fn approve_block() {
        let mut engine = init(vec![
            NodeId::new([0; 32]),
            NodeId::new([1; 32]),
            NodeId::new([3; 32]),
        ]);

        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block.clone()).unwrap();
        engine = update_leader_selection(&engine);

        let (engine, send) = engine.approve_block(block.clone());
        assert_eq!(engine.highest_voted_view, block.view);
        assert_eq!(
            send.to,
            vec![engine.overlay().next_leader()].into_iter().collect()
        );
        assert_eq!(
            send.payload,
            Payload::Vote(Vote {
                block: block.id,
                view: block.view
            })
        );
    }

    #[test]
    #[should_panic(expected = "not in")]
    // Ensure that approve_block cannot accept not-received blocks.
    fn approve_block_not_received() {
        let engine = init(vec![NodeId::new([0; 32])]);

        let block = next_block(&engine, &engine.genesis_block());
        let _ = engine.approve_block(block);
    }

    #[test]
    #[should_panic(expected = "can't vote for a block in the past")]
    // Ensure that approve_block cannot vote blocks in the past.
    fn approve_block_in_the_past() {
        let mut engine = init(vec![NodeId::new([0; 32])]);

        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block.clone()).unwrap();
        engine = update_leader_selection(&engine);

        let (engine, _) = engine.approve_block(block.clone());

        // trying to approve the block again that was already voted.
        let _ = engine.approve_block(block);
    }

    #[test]
    // Ensure that local_timeout() votes on the current view.
    fn local_timeout() {
        let mut engine = init(vec![NodeId::new([0; 32]), NodeId::new([1; 32])]);
        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block).unwrap(); // received but not approved yet
        engine = update_leader_selection(&engine);

        let (engine, send) = engine.local_timeout();
        assert_eq!(engine.highest_voted_view, View(1)); // updated from 0 (genesis) to 1 (current_view)
        assert_eq!(
            send,
            Some(Send {
                to: engine.overlay().root_committee(),
                payload: Payload::Timeout(Timeout {
                    view: View(1),
                    sender: NodeId::new([0; 32]),
                    high_qc: StandardQc {
                        view: View(0), // genesis
                        id: 0,
                    },
                    timeout_qc: None
                }),
            }),
        );
    }

    #[test]
    // Ensure that receive_timeout_qc updates current_view, last_view_timeout_qc and local_high_qc.
    fn receive_timeout_qc_after_local_timeout() {
        let mut engine = init(vec![NodeId::new([0; 32])]);
        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block).unwrap(); // received but not approved yet
        engine = update_leader_selection(&engine);

        let (mut engine, _) = engine.local_timeout();

        assert_eq!(engine.current_view(), View(1));
        let timeout_qc = TimeoutQc::new(
            View(1),
            StandardQc {
                view: View::new(0), // genesis
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        engine = engine.receive_timeout_qc(timeout_qc.clone());
        assert_eq!(&engine.local_high_qc, timeout_qc.high_qc());
        assert_eq!(engine.last_view_timeout_qc, Some(timeout_qc));
        assert_eq!(engine.current_view(), View(2));
    }

    #[test]
    // Ensure that receive_timeout_qc works even before local_timeout occurs.
    fn receive_timeout_qc_before_local_timeout() {
        let mut engine = init(vec![NodeId::new([0; 32])]);
        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block).unwrap(); // received but not approved yet
        engine = update_leader_selection(&engine);

        // before local_timeout occurs

        assert_eq!(engine.current_view(), View(1));
        let timeout_qc = TimeoutQc::new(
            View(1),
            StandardQc {
                view: View(0), // genesis
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        engine = engine.receive_timeout_qc(timeout_qc.clone());
        assert_eq!(&engine.local_high_qc, timeout_qc.high_qc());
        assert_eq!(engine.last_view_timeout_qc, Some(timeout_qc));
        assert_eq!(engine.current_view(), View(2));
    }

    #[test]
    // Ensure that approve_new_view votes on the new view correctly.
    fn approve_new_view() {
        let mut engine = init(vec![
            NodeId::new([0; 32]),
            NodeId::new([1; 32]),
            NodeId::new([2; 32]),
        ]);
        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block).unwrap(); // received but not approved yet
        engine = update_leader_selection(&engine);

        assert_eq!(engine.current_view(), View(1)); // still waiting for a QC(view=1)
        let timeout_qc = TimeoutQc::new(
            View(1),
            StandardQc {
                view: View(0), // genesis
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        engine = engine.receive_timeout_qc(timeout_qc.clone());
        assert_eq!(&engine.local_high_qc, timeout_qc.high_qc());
        assert_eq!(engine.last_view_timeout_qc, Some(timeout_qc.clone()));
        assert_eq!(engine.current_view(), View(2));
        assert_eq!(engine.highest_voted_view, View(-1)); // didn't vote on anything yet
        engine = update_leader_selection(&engine);

        let (engine, send) = engine.approve_new_view(timeout_qc.clone(), HashSet::new());
        assert_eq!(&engine.high_qc(), timeout_qc.high_qc());
        assert_eq!(engine.current_view(), View(2)); // not changed
        assert_eq!(engine.highest_voted_view, View(2));
        assert_eq!(
            send,
            Send {
                to: vec![engine.overlay().next_leader()].into_iter().collect(),
                payload: Payload::NewView(NewView {
                    view: View(2),
                    sender: NodeId::new([0; 32]),
                    timeout_qc: timeout_qc.clone(),
                    high_qc: timeout_qc.high_qc().clone(),
                })
            }
        );
    }

    #[test]
    #[should_panic(expected = "can't vote for a new view not bigger than the last timeout_qc")]
    fn approve_new_view_not_bigger_than_timeout_qc() {
        let mut engine = init(vec![NodeId::new([0; 32])]);
        let block = next_block(&engine, &engine.genesis_block());
        engine = engine.receive_block(block).unwrap(); // received but not approved yet
        engine = update_leader_selection(&engine);

        assert_eq!(engine.current_view(), View(1));
        let timeout_qc1 = TimeoutQc::new(
            View(1),
            StandardQc {
                view: View(0), // genesis
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        engine = engine.receive_timeout_qc(timeout_qc1.clone());
        assert_eq!(engine.last_view_timeout_qc, Some(timeout_qc1.clone()));
        engine = update_leader_selection(&engine);

        // receiving a timeout_qc2 before approving new_view(timeout_qc1)
        let timeout_qc2 = TimeoutQc::new(
            View(2),
            StandardQc {
                view: View(0), // genesis
                id: 0,
            },
            NodeId::new([0; 32]),
        );
        engine = engine.receive_timeout_qc(timeout_qc2.clone());
        assert_eq!(engine.last_view_timeout_qc, Some(timeout_qc2));
        engine = update_leader_selection(&engine);

        // we expect new_view(timeout_qc2), but...
        let _ = engine.approve_new_view(timeout_qc1, HashSet::new());
    }
}
