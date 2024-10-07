use std::collections::VecDeque;

/// A [`Queue`] controls the order of messages to be emitted to a single connection.
pub trait Queue<T> {
    /// Push a message to the queue.
    fn push(&mut self, data: T);

    /// Pop a message from the queue.
    fn pop(&mut self) -> Option<T>;
}

/// A regular queue that does not mix the order of messages.
///
/// This queue returns a noise message if the queue is empty.
pub struct NonMixQueue<T> {
    queue: VecDeque<T>,
}

impl<T> NonMixQueue<T> {
    pub fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }
}

impl<T> Default for NonMixQueue<T> {
    fn default() -> Self {
        Self::new()
    }
}

impl<T> Queue<T> for NonMixQueue<T> {
    fn push(&mut self, data: T) {
        self.queue.push_back(data);
    }

    fn pop(&mut self) -> Option<T> {
        self.queue.pop_front()
    }
}
