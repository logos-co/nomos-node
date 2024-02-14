use std::time::Duration;

use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Exp};

/// The Poisson distribution
///
/// The Poisson distribution is used because of its memoryless properties,
/// which is known to offer optimal anonymity properties (Nym whitepaper 4.4).
#[derive(Copy, Clone, Debug)]
pub struct Poisson<R>(R);

impl<R: SeedableRng> Poisson<R> {
    /// Returns a new instance
    pub fn new(seed: R::Seed) -> Self {
        Self(R::from_seed(seed))
    }
}

impl<R: Rng> Poisson<R> {
    /// Get a random interval between events that follow a Poisson distribution.
    ///
    /// If events occur in a Poisson distribution with rate_per_min,
    /// the interval between events follow the exponential distribution with rate_per_min.
    pub fn interval(&mut self, rate_per_min: f64) -> Duration {
        // create an exponential distribution
        let exp = Exp::new(rate_per_min).unwrap();
        // generate a random value from the distribution
        let interval_min = exp.sample(&mut self.0);
        // convert minutes to seconds
        Duration::from_secs_f64(interval_min * 60.0)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::StdRng;
    use std::{collections::BTreeMap, time::Duration};

    // Test the creation of a PoissonExpInterval instance
    #[test]
    fn test_poisson_exp_interval_creation() {
        let seed = [0u8; 32]; // A seed for StdRng
        let _instance = Poisson::<StdRng>::new(seed);
        // If no panic occurs, the test passes
    }

    // Test the interval generation for a specific rate
    #[test]
    fn test_interval_generation() {
        let seed = [0u8; 32]; // A seed for StdRng
        let mut instance = Poisson::<StdRng>::new(seed);
        let rate_per_min = 1.0; // 1 event per minute
        let interval = instance.interval(rate_per_min);
        // Check if the interval is within a plausible range
        // This is a basic check; in practice, you may want to perform a statistical test
        assert!(interval > Duration::from_secs(0)); // Must be positive
    }

    // Compute the empirical CDF
    fn empirical_cdf(samples: &[Duration]) -> BTreeMap<Duration, f64> {
        let mut map = BTreeMap::new();
        let n = samples.len() as f64;

        for &sample in samples {
            *map.entry(sample).or_insert(0.0) += 1.0 / n;
        }

        let mut acc = 0.0;
        for value in map.values_mut() {
            acc += *value;
            *value = acc;
        }

        map
    }

    // Compare the empirical CDF to the theoretical CDF
    #[test]
    fn test_distribution_fit() {
        let seed = [1u8; 32];
        let mut instance = Poisson::<StdRng>::new(seed);
        let rate_per_min = 1.0;
        let mut intervals = Vec::new();

        // Generate 10,000 samples
        for _ in 0..10_000 {
            intervals.push(instance.interval(rate_per_min));
        }

        let empirical = empirical_cdf(&intervals);

        // theoretical CDF for exponential distribution
        let rate_per_sec = rate_per_min / 60.0;
        let theoretical_cdf = |x: f64| 1.0 - (-rate_per_sec * x).exp();

        // Kolmogorov-Smirnov test
        let ks_statistic: f64 = empirical
            .iter()
            .map(|(&k, &v)| {
                let x = k.as_secs_f64();
                (theoretical_cdf(x) - v).abs()
            })
            .fold(0.0, f64::max);

        println!("KS Statistic: {}", ks_statistic);

        assert!(ks_statistic < 0.05, "Distributions differ significantly.");
    }
}
