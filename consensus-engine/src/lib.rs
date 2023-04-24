use std::collections::{HashMap, HashSet};

mod io;
use io::*;
use io::Output::Send;

pub type View = i64;
pub type Id = [u8; 32];

#[derive(Clone, Debug)]
pub struct CarnotState<O: Overlay> {
    id: Id,
    current_view: View,
    highest_voted_view: View,
    local_high_qc: Qc,
    safe_blocks: HashMap<Id, Block>,
    last_view_timeout_qc: Qc,
    overlay: O,
}

pub trait Overlay: Clone {
    fn root_committee(&self) -> Id;
    fn rebuild(&mut self, timeout_qc: Qc);
    fn is_member_of_child_committee(&self, id: Id) -> bool;
    fn is_member_of_root_committee(&self, id: Id) -> bool;
    fn is_child_of_root_committee(&self, id: Id) -> bool;
    fn supermajority_threshold(&self, id: Id) -> usize;
    fn parent_committee(&self, id: Id) -> Id;
    fn leader(&self, view: View) -> Id;
}

pub struct ConsensusEngine<O: Overlay> {
    state: CarnotState<O>,
}

struct Vote {
    block: Id,
    voter: Id,
}

struct Timeout {
    view: View,
    sender: Id,
    high_qc: Qc,
}

struct NewView {
    view: View,
    sender: Id,
    timeout_qc: Qc,
    high_qc: Qc,
}

impl<O: Overlay> ConsensusEngine<O> {
    pub fn from_genesis(id: Id, genesis_block: Block, overlay: O) -> Self {
        Self {
            state: CarnotState {
                current_view: 0,
                // TODO: fix
                local_high_qc: Qc::Standard {
                    view: -1,
                    id: genesis_block.id,
                },
                id,
                highest_voted_view: -1,
                // TODO: fix
                last_view_timeout_qc: Qc::Standard {
                    view: -1,
                    id: genesis_block.id,
                },
                overlay,
                safe_blocks: [(id, genesis_block)].into(),
            },
        }
    }

    fn update_state_on_success(
        &mut self,
        next: Result<(CarnotState<O>, Vec<Output>), ()>,
    ) -> Vec<Output> {
        match next {
            Ok((next_state, out)) => {
                self.state = next_state;
                out
            }
            Err(()) => vec![],
        }
    }

    pub fn step(&mut self, input: Input) -> Vec<Output> {
        let maybe_next_state = match input {
            Input::Block { block } => self.state.receive_block(block),
            Input::Timeout => self.state.local_timeout(),
            _ => unimplemented!(),
        };
        self.update_state_on_success(maybe_next_state)
    }

    pub fn committed_blocks(&self) -> Vec<Id> {
        todo!()
    }
}

impl<O: Overlay> CarnotState<O> {
    fn receive_block(&self, block: Block) -> Result<(Self, Vec<Output>), ()> {
        assert!(
            self.safe_blocks.contains_key(&block.parent()),
            "out of order view not supported, missing parent block for {:?}",
            block
        );

        // if the block has already been processed, return early
        if self.safe_blocks.contains_key(&block.id) {
            return Err(());
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

        if new_state.block_is_safe(block) {
            new_state.safe_blocks.insert(block.id, block);
            new_state.update_high_qc(block.parent_qc);
        }

        Ok((new_state, vec![]))
    }

    fn receive_timeout_qc(&self, timeout_qc: Qc) {
        assert!(timeout_qc.view() > self.current_view);
        // self.update_high_qc(timeout_qc.high_qc);
        self.update_timeout_qc(timeout_qc);

        self.current_view += 1;
        self.overlay.rebuild(timeout_qc);
    }

    // TODO: list preconditions
    fn approve_block(&self, block: Block) -> Output {
        assert!(self.safe_blocks.contains_key(&block.id));
        assert!(
            self.highest_voted_view < block.view,
            "can't vote for a block in the past"
        );

        self.highest_voted_view = block.view;
        if self.overlay.is_member_of_root_committee(self.id) {
            return Send {
                to: self.overlay.leader(block.view + 1),
                payload: Box::new(Vote {
                    block: block.id,
                    voter: self.id,
                }),
            };
        }
        return Send {
            to: self.overlay.parent_committee(self.id),
            payload: Box::new(Vote {
                block: block.id,
                voter: self.id,
            }),
        };
    }

    // TODO: list preconditions
    fn approve_new_view(&self, timeout_qc: Qc, new_views: HashSet<NewView>) -> Send<NewView> {
        let new_view = timeout_qc.view() + 1;
        assert!(new_view >= self.last_view_timeout_qc.view());
        assert_eq!(
            new_views.len(),
            self.overlay.supermajority_threshold(self.id)
        );
        assert!(new_views
            .iter()
            .all(|nv| self.overlay.is_member_of_child_committee(nv.sender)));
        assert!(self.highest_voted_view < new_view);
        assert!(new_views.iter().all(|nv| nv.view == new_view));

        let high_qc = new_views
            .iter()
            .map(|nv| nv.high_qc)
            .chain(std::iter::once(timeout_qc))
            .max_by_key(|qc| qc.view())
            .unwrap();
        self.update_high_qc(high_qc);

        let new_view_msg = NewView {
            view: new_view,
            high_qc,
            sender: self.id,
            timeout_qc,
        };
        self.highest_voted_view = new_view;

        return Send {
            to: if self.overlay.is_member_of_root_committee(self.id) {
                self.overlay.leader(new_view + 1)
            } else {
                self.overlay.parent_committee(self.id)
            },
            payload: Box::new(new_view_msg),
        };
    }

    fn local_timeout(&self) -> Result<(Self, Vec<Output>), ()> {
        self.highest_voted_view = self.current_view;
        if self.overlay.is_member_of_root_committee(self.id)
            || self.overlay.is_child_of_root_committee(self.id)
        {
            let timeout_msg = Timeout {
                view: self.current_view,
                high_qc: self.local_high_qc,
                sender: self.id,
            };
            return Ok((
                self.clone(),
                vec![Send {
                    to: self.overlay.root_committee(),
                    payload: Box::new(timeout_msg),
                }],
            ));
        }
        Ok((self.clone(), vec![]))
    }

    fn block_is_safe(&self, block: Block) -> bool {
        return block.view >= self.current_view && block.view == block.parent_qc.view() + 1;
    }

    fn update_high_qc(&mut self, qc: Qc) {
        match qc {
            Qc::Standard { view, id: _ } if view > self.local_high_qc.view() => {
                self.local_high_qc = qc;
            }
            Qc::Aggregated { high_qc_view, .. } if high_qc_view != self.local_high_qc.view() => {
                self.local_high_qc = qc;
            }
            _ => {}
        }
        if qc.view() == self.current_view {
            self.current_view += 1;
        }
    }

    fn update_timeout_qc(&mut self, timeout_qc: Qc) {
        if self.last_view_timeout_qc.view() < timeout_qc.view() {
            self.last_view_timeout_qc = timeout_qc;
        }
    }

    fn blocks_in_view(&self, view: View) -> Vec<Block> {
        self.safe_blocks
            .iter()
            .filter(|(_, b)| b.view == view)
            .map(|(_, block)| *block)
            .collect()
    }

    fn genesis_block(&self) -> Block {
        self.blocks_in_view(0)[0]
    }

    // Returns the id of the grandparent block if it can be committed or None otherwise
    fn can_commit_grandparent(&self, block: Block) -> Option<Block> {
        let parent = self.safe_blocks.get(&block.parent())?;
        let grandparent = self.safe_blocks.get(&parent.parent())?;

        if parent.view == grandparent.view + 1
            && matches!(parent.parent_qc, Qc::Standard { .. })
            && matches!(grandparent.parent_qc, Qc::Standard { .. })
        {
            return Some(*grandparent);
        }
        None
    }

    fn latest_committed_block(&self) -> Block {
        for view in (0..self.current_view).rev() {
            for block in self.blocks_in_view(view) {
                if let Some(block) = self.can_commit_grandparent(block) {
                    return block;
                }
            }
        }
        self.genesis_block()
    }

    fn latest_committed_view(&self) -> View {
        self.latest_committed_block().view
    }

    fn committed_blocks(&self) -> Vec<Id> {
        let mut res = vec![];
        let mut current = self.latest_committed_block();
        while current != self.genesis_block() {
            res.push(current.id);
            current = *self.safe_blocks.get(&current.parent()).unwrap();
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

    #[test]
    fn block_is_safe() {
        let mut engine = ConsensusEngine::from_genesis(
            [0; 32],
            Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        );
        let block = Input::Block {
            view: 1,
            id: [1; 32],
            parent_qc: Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        };
        let out = engine.step(block);
        assert_eq!(
            out,
            vec![Output::Safeblock {
                view: 1,
                id: [1; 32]
            }]
        );
    }

    #[test]
    fn block_is_committed() {
        let mut engine = ConsensusEngine::from_genesis(
            [0; 32],
            Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        );
        let p1 = Input::block {
            view: 1,
            id: [1; 32],
            parent_qc: Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        };
        let p2 = Input::block {
            view: 2,
            id: [2; 32],
            parent_qc: Qc::Standard {
                view: 1,
                id: [1; 32],
            },
        };
        let _ = engine.step(p1);
        println!("step 1");
        let out = engine.step(p2);
        println!("step 1");
        assert_eq!(
            out,
            vec![
                Output::Safeblock {
                    view: 2,
                    id: [2; 32]
                },
                Output::Committed {
                    view: 0,
                    id: [0; 32]
                }
            ]
        );
    }
}
