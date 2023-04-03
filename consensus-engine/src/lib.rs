use std::collections::HashMap;

mod io;
use io::{Input, Output, Qc};

pub type View = u64;
pub type Id = [u8; 32];

#[derive(Clone, Debug)]
pub struct CarnotState {
    view: u64,
    high_qc: u64,
    last_committed_view: u64,
    // proposal id -> (view, parent_qc)
    proposals: HashMap<Id, (View, Qc)>,
}

pub struct ConsensusEngine {
    state: CarnotState,
}

impl ConsensusEngine {
    pub fn from_genesis(id: Id, qc: Qc) -> Self {
        Self {
            state: CarnotState {
                view: 0,
                high_qc: 0,
                last_committed_view: 0,
                proposals: [(id, (0, qc))].into(),
            },
        }
    }

    fn update_state_on_success(
        &mut self,
        next: Result<(CarnotState, Vec<Output>), ()>,
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
            Input::Proposal {
                view,
                id,
                parent_qc,
            } => self.state.process_proposal(view, id, parent_qc),
            Input::Timeout => self.state.process_timeout(),
            _ => unimplemented!(),
        };
        self.update_state_on_success(maybe_next_state)
    }
}

impl CarnotState {
    fn process_proposal(
        &self,
        view: View,
        id: Id,
        parent_qc: Qc,
    ) -> Result<(Self, Vec<Output>), ()> {
        assert!(
            self.proposals.contains_key(&parent_qc.block()),
            "out of order view not supported {:?}",
            self.proposals
        );

        // if the proposal has already been processed, or if it's not safe, return
        if self.proposals.contains_key(&id) || !self.safe_proposal(view, parent_qc) {
            return Err(());
        }

        let mut new_state = self.clone();

        let mut res = vec![Output::SafeProposal { view, id }];

        new_state.update_high_qc(parent_qc);
        new_state.view += 1;

        new_state.proposals.insert(id, (view, parent_qc));
        res.extend(new_state.try_commit(view, parent_qc));

        Ok((new_state, res))
    }

    fn process_timeout(&self) -> Result<(Self, Vec<Output>), ()> {
        todo!()
    }

    fn safe_proposal(&self, view: View, parent_qc: Qc) -> bool {
        // FIXME: there should be a cleaner condition for the first proposal
        if self.view == 0 && parent_qc.view() == 0 && view == 1 {
            return true;
        }

        if parent_qc.view() <= self.last_committed_view || view <= self.view {
            return false;
        }

        // This in theory could be an invariant of the block/proposal type, but let's put
        // it here for now
        if let Qc::Standard {
            view: parent_view, ..
        } = parent_qc
        {
            return view == parent_view + 1;
        }

        true
    }

    fn update_high_qc(&mut self, qc: Qc) {
        match qc {
            Qc::Standard { view, id: _ } if view > self.high_qc => {
                self.high_qc = view;
            }
            Qc::Aggregated { high_qc_view, .. } if high_qc_view != self.high_qc => {
                self.high_qc = high_qc_view;
            }
            _ => {}
        }
    }

    fn try_commit(&mut self, _view: View, qc: Qc) -> Option<Output> {
        let Qc::Standard{view: _, id: parent} = qc else{
            return None;
        };

        let Some(&(parent_view,  Qc::Standard{view: grandparent_view, id: grandparent})) = self.proposals.get(&parent)
          else {
            return None;
        };

        let pipelined = parent_view == grandparent_view + 1;
        if pipelined {
            self.last_committed_view = grandparent_view;
            return Some(Output::Committed {
                view: grandparent_view,
                id: grandparent,
            });
        }
        None
    }
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn proposal_is_safe() {
        let mut engine = ConsensusEngine::from_genesis(
            [0; 32],
            Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        );
        let proposal = Input::Proposal {
            view: 1,
            id: [1; 32],
            parent_qc: Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        };
        let out = engine.step(proposal);
        assert_eq!(
            out,
            vec![Output::SafeProposal {
                view: 1,
                id: [1; 32]
            }]
        );
    }

    #[test]
    fn proposal_is_committed() {
        let mut engine = ConsensusEngine::from_genesis(
            [0; 32],
            Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        );
        let p1 = Input::Proposal {
            view: 1,
            id: [1; 32],
            parent_qc: Qc::Standard {
                view: 0,
                id: [0; 32],
            },
        };
        let p2 = Input::Proposal {
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
                Output::SafeProposal {
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
