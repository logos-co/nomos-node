use std::time::Duration;

use rand::Rng;
use rand_distr::{Distribution, Exp};

/// Get a random interval between events that follow a Poisson distribution.
///
/// If events occur in a Poisson distribution with rate_per_min,
/// the interval between events follow the exponential distribution with rate_per_min.
pub fn poisson_interval<R: Rng + ?Sized>(rng: &mut R, rate_per_min: f64) -> Duration {
    // create an exponential distribution
    let exp = Exp::new(rate_per_min).unwrap();
    // generate a random value from the distribution
    let interval_min = exp.sample(rng);
    // convert minutes to seconds
    Duration::from_secs_f64(interval_min * 60.0)
}

#[cfg(test)]
mod tests {
    use super::*;
    use rand::rngs::OsRng;
    use std::{collections::BTreeMap, time::Duration};

    // Test the interval generation for a specific rate
    #[test]
    fn test_interval_generation() {
        let interval = poisson_interval(&mut OsRng, 1.0);
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
        let rate_per_min = 1.0;
        let mut intervals = Vec::new();

        // Generate 10,000 samples
        for _ in 0..10_000 {
            intervals.push(poisson_interval(&mut OsRng, rate_per_min));
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
