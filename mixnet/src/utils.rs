use std::time::Duration;

use rand::{Rng, SeedableRng};
use rand_distr::{Distribution, Exp};

/// A struct which helps give us a random interval from an exponential distribution.
#[derive(Copy, Clone, Debug)]
pub struct PoissonExpInterval<R>(R);

impl<R: SeedableRng> PoissonExpInterval<R> {
    /// Returns a new instance
    pub fn new(seed: R::Seed) -> Self {
        Self(R::from_seed(seed))
    }
}

impl<R: Rng> PoissonExpInterval<R> {
    /// Get a random interval from an exponential distribution in seconds.
    ///
    /// The Poisson (exponential) distribution is used because of its memoryless properties,
    /// which is known to offer optimal anonymity properties (Nym whitepaper 4.4).
    pub fn interval(&mut self, rate_per_min: f64) -> Duration {
        // create an exponential distribution
        let exp = Exp::new(rate_per_min).unwrap();
        // generate a random value from the distribution
        let interval_min = exp.sample(&mut self.0);
        // convert minutes to seconds
        Duration::from_secs_f64(interval_min * 60.0)
    }

    /// Get the mean interval in seconds for a Poisson.
    pub fn mean_interval(rate_per_min: f64) -> Duration {
        // the mean interval in seconds for a Poisson process
        Duration::from_secs_f64(1.0 / rate_per_min * 60.0)
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
        let _instance = PoissonExpInterval::<StdRng>::new(seed);
        // If no panic occurs, the test passes
    }

    // Test the interval generation for a specific rate
    #[test]
    fn test_interval_generation() {
        let seed = [0u8; 32]; // A seed for StdRng
        let mut instance = PoissonExpInterval::<StdRng>::new(seed);
        let rate_per_min = 1.0; // 1 event per minute
        let interval = instance.interval(rate_per_min);
        // Check if the interval is within a plausible range
        // This is a basic check; in practice, you may want to perform a statistical test
        assert!(interval > Duration::from_secs(0)); // Must be positive
    }

    // Test the mean interval calculation
    #[test]
    fn test_mean_interval() {
        let rate_per_min = 1.0; // 1 event per minute
        let expected_mean = Duration::from_secs(60); // 60 seconds
        let mean_interval = PoissonExpInterval::<StdRng>::mean_interval(rate_per_min);
        assert_eq!(mean_interval, expected_mean);
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
        let mut rng = StdRng::from_seed(seed);
        let rate_per_min = 1.0;
        let lambda = rate_per_min / 60.0;
        let mut intervals = Vec::new();

        // Generate 10,000 samples
        for _ in 0..10_000 {
            let exp = Exp::new(lambda).unwrap();
            let interval_sec = exp.sample(&mut rng);
            intervals.push(Duration::from_secs_f64(interval_sec));
        }

        let empirical = empirical_cdf(&intervals);

        // theoretical CDF for exponential distribution
        let theoretical_cdf = |x: f64| 1.0 - (-lambda * x).exp();

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
