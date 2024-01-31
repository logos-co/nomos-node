use std::time::Duration;

use rand::{thread_rng, Rng, RngCore, SeedableRng};
use rand_distr::{Distribution, Exp};

/// Get a random interval from an exponential distribution.
pub fn poisson_interval_sec(rate_per_min: f64) -> Duration {
    // create an exponential distribution
    let exp = Exp::new(rate_per_min).unwrap();
    // generate a random value from the distribution
    let interval_min = exp.sample(&mut thread_rng());
    // convert minutes to seconds
    Duration::from_secs_f64(interval_min * 60.0)
}

/// Get the mean interval in seconds for a Poisson.
pub fn poisson_mean_interval_sec(rate_per_min: f64) -> Duration {
    // the mean interval in seconds for a Poisson process
    Duration::from_secs_f64(1.0 / rate_per_min * 60.0)
}

/// Fisher-Yates shuffling algorithm.
pub struct FisherYates<R: SeedableRng>(std::marker::PhantomData<R>);

impl<R: SeedableRng> Default for FisherYates<R> {
    fn default() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<R: SeedableRng> Clone for FisherYates<R> {
    fn clone(&self) -> Self {
        *self
    }
}

impl<R: SeedableRng> Copy for FisherYates<R> {}

impl<R: SeedableRng> FisherYates<R> {
    /// Create a new instance.
    #[inline(always)]
    pub const fn new() -> Self {
        Self(std::marker::PhantomData)
    }
}

impl<R: SeedableRng + RngCore> FisherYates<R> {
    /// Shuffle the given elements in place.
    pub fn shuffle<T>(elements: &mut [T], entropy: R::Seed) {
        let mut rng = R::from_seed(entropy);

        for i in (1..elements.len()).rev() {
            let j = rng.gen_range(0..=i);
            elements.swap(i, j);
        }
    }
}
