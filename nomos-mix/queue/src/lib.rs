use std::collections::VecDeque;

/// A [`Queue`] controls the order of messages to be emitted to a single connection.
pub trait Queue<T> {
    /// Push a message to the queue.
    fn push(&mut self, data: T);

    /// Pop a message from the queue.
    ///
    /// The returned message is either the real message pushed before or a noise message.
    fn pop(&mut self) -> T;
}

/// A regular queue that does not mix the order of messages.
///
/// This queue returns a noise message if the queue is empty.
pub struct NonMixQueue<T: Clone> {
    queue: VecDeque<T>,
    noise: T,
}

impl<T: Clone> NonMixQueue<T> {
    pub fn new(noise: T) -> Self {
        Self {
            queue: VecDeque::new(),
            noise,
        }
    }
}

impl<T: Clone> Queue<T> for NonMixQueue<T> {
    fn push(&mut self, data: T) {
        self.queue.push_back(data);
    }

    fn pop(&mut self) -> T {
        self.queue.pop_front().unwrap_or(self.noise.clone())
    }
}
