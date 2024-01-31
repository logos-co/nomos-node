use std::time::Duration;

use rand::thread_rng;
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
