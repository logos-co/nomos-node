use rand::{rngs::StdRng, seq::SliceRandom, Rng, SeedableRng};

use crate::Queue;

pub struct PermutedCoinFlippingQueue<T: Clone> {
    queue: Vec<T>,
    rng: StdRng,
    min_pool_size: usize,
    noise: T,
}

impl<T: Clone> PermutedCoinFlippingQueue<T> {
    pub fn new(min_pool_size: usize, noise: T) -> Self {
        Self {
            queue: vec![noise.clone(); min_pool_size],
            rng: StdRng::from_entropy(),
            min_pool_size,
            noise,
        }
    }

    fn fill_noises(&mut self, k: usize) {
        self.queue
            .extend(std::iter::repeat(self.noise.clone()).take(k))
    }

    fn flip_coin(&mut self) -> bool {
        self.rng.gen_bool(0.5)
    }
}

impl<T: Clone> Queue<T> for PermutedCoinFlippingQueue<T> {
    fn push(&mut self, data: T) {
        self.queue.push(data);
    }

    fn pop(&mut self) -> T {
        if self.queue.len() < self.min_pool_size {
            self.fill_noises(self.min_pool_size - self.queue.len());
        }

        self.queue.as_mut_slice().shuffle(&mut self.rng);

        loop {
            for i in 0..self.queue.len() {
                if self.flip_coin() {
                    return self.queue.swap_remove(i);
                }
            }
        }
    }
}
