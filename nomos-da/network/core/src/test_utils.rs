use libp2p::PeerId;
use std::collections::HashSet;
use subnetworks_assignations::MembershipHandler;

#[derive(Clone)]
pub struct AllNeighbours {
    pub neighbours: HashSet<PeerId>,
}

impl MembershipHandler for AllNeighbours {
    type NetworkId = u32;
    type Id = PeerId;

    fn membership(&self, _self_id: &Self::Id) -> HashSet<Self::NetworkId> {
        [0].into_iter().collect()
    }

    fn is_allowed(&self, _id: &Self::Id) -> bool {
        true
    }

    fn members_of(&self, _network_id: &Self::NetworkId) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }

    fn members(&self) -> HashSet<Self::Id> {
        self.neighbours.clone()
    }
}

use crossbeam_channel::{bounded, Receiver, Sender};
use std::sync::{Arc, Mutex};

#[derive(Debug)]
/// A special-purpose multi-producer, multi-consumer(MPMC) channel to relay messages indicating whether the associated stream should be closed or not. This channel is intended to be used on sampling, dispersal and replication protocol tests to ensure graceful shutdown of streams after event has completed.
pub struct ConnectionClosingHandshake {
    pub sender: Sender<bool>,
    pub receiver: Receiver<bool>,
    pub done: Arc<Mutex<bool>>,
}

impl ConnectionClosingHandshake {
    pub fn new(size: usize) -> Self {
        let (sender, receiver) = bounded(size);
        Self {
            sender,
            receiver,
            done: Arc::new(Mutex::new(false)),
        }
    }

    pub fn send(&self) {
        let message = *self.done.lock().unwrap();
        self.sender.send(message).unwrap();
    }

    pub fn receive(&self) -> Option<bool> {
        self.receiver.try_recv().ok()
    }

    pub fn set_done(&self) {
        let mut guard = self.done.lock().unwrap();
        *guard = true;
    }
}

impl Clone for ConnectionClosingHandshake {
    fn clone(&self) -> Self {
        Self {
            sender: self.sender.clone(),
            receiver: self.receiver.clone(),
            done: Arc::clone(&self.done),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::thread;
    use std::time::Duration;

    #[test]
    fn test_connection_closing_handshake_with_concurrent_loops() {
        let handshake = ConnectionClosingHandshake::new(1);
        let num_ongoing_messages = 5;
        let handshake_for_ongoing = handshake.clone();

        let ongoing_thread = thread::spawn(move || {
            for _ in 0..num_ongoing_messages {
                handshake_for_ongoing.send();
            }
        });

        let handshake_for_done = handshake.clone();
        let done_thread = thread::spawn(move || {
            // Wait briefly before setting done to allow some "Ongoing" messages to be sent.
            thread::sleep(Duration::from_millis(5));
            handshake_for_done.set_done();
            handshake_for_done.send();
        });
        let mut i = 0;
        loop {
            if let Some(message) = handshake.receive() {
                if i != 5 {
                    assert_eq!(
                        message, false,
                        "Expected 'Ongoing' (false) message at index {}",
                        i
                    );
                    i += 1;
                } else {
                    assert_eq!(
                        message, true,
                        "Expected 'Finished' (true) message after all 'Ongoing' messages"
                    );
                    break;
                }
            }
        }
        ongoing_thread.join().unwrap();
        done_thread.join().unwrap();
    }
}
