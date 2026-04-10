//! Live per-pool volatility tracking from block-to-block depth deltas.

use std::collections::{HashMap, VecDeque};

use num_bigint::BigUint;
use num_traits::ToPrimitive;

use crate::derived::types::PoolDepthKey;

/// Per-pool volatility state: ring buffer of relative depth deltas with cached statistics.
#[derive(Debug, Clone)]
struct PoolVolatility {
    prev_depth: Option<f64>,
    deltas: VecDeque<f64>,
    sample_count: usize,
    cached_sigma: f64,
    cached_variance: f64,
}

impl PoolVolatility {
    fn new() -> Self {
        Self {
            prev_depth: None,
            deltas: VecDeque::new(),
            sample_count: 0,
            cached_sigma: 0.0,
            cached_variance: 0.0,
        }
    }

    /// Pushes a new relative delta and recomputes weighted statistics.
    fn push_delta(&mut self, delta: f64, alpha: f64, max_window: usize) {
        self.deltas.push_back(delta);
        self.sample_count += 1;
        if self.deltas.len() > max_window {
            self.deltas.pop_front();
        }
        self.recompute_stats(alpha);
    }

    /// Exponentially-weighted mean and variance over the ring buffer.
    /// Most recent observation has weight 1.0, each prior multiplied by α.
    fn recompute_stats(&mut self, alpha: f64) {
        let n = self.deltas.len();
        if n == 0 {
            self.cached_sigma = 0.0;
            self.cached_variance = 0.0;
            return;
        }

        let mut weight_sum = 0.0;
        let mut weighted_mean = 0.0;

        // Iterate oldest to newest; newest gets weight 1.0
        for (i, &d) in self.deltas.iter().enumerate() {
            let age = (n - 1 - i) as f64;
            let w = alpha.powf(age);
            weight_sum += w;
            weighted_mean += w * d;
        }
        weighted_mean /= weight_sum;

        let mut weighted_var = 0.0;
        for (i, &d) in self.deltas.iter().enumerate() {
            let age = (n - 1 - i) as f64;
            let w = alpha.powf(age);
            let diff = d - weighted_mean;
            weighted_var += w * diff * diff;
        }
        weighted_var /= weight_sum;

        self.cached_sigma = weighted_var.sqrt();
        self.cached_variance = weighted_var;
    }
}

/// Configuration for the volatility tracker, specified in seconds.
/// Converted to block counts at runtime based on observed block time.
#[derive(Debug, Clone)]
pub struct VolatilityTrackerConfig {
    /// Observation window in seconds (e.g. 300 = 5 min).
    pub window_secs: u64,
    /// Minimum observation time in seconds before trusting σ_F (e.g. 60 = 1 min).
    pub min_samples_secs: u64,
    /// Exponential decay factor. 0.95 = block from 50 ago has ~8% weight.
    pub alpha: f64,
    /// Observed block time in seconds (e.g. 12 for Ethereum mainnet).
    pub block_time_secs: u64,
}

impl Default for VolatilityTrackerConfig {
    fn default() -> Self {
        Self { window_secs: 300, min_samples_secs: 60, alpha: 0.95, block_time_secs: 12 }
    }
}

impl VolatilityTrackerConfig {
    fn max_window(&self) -> usize {
        (self.window_secs / self.block_time_secs.max(1)) as usize
    }

    fn min_samples(&self) -> usize {
        (self.min_samples_secs / self.block_time_secs.max(1)) as usize
    }
}

/// Accumulates block-to-block depth deltas per pool and exposes per-pool σ_F.
///
/// Shared across workers via `Arc<RwLock<VolatilityTracker>>`.
/// Idempotent: repeated calls with the same block are no-ops.
#[derive(Debug, Clone)]
pub struct VolatilityTracker {
    pools: HashMap<PoolDepthKey, PoolVolatility>,
    config: VolatilityTrackerConfig,
    last_block: Option<u64>,
}

impl VolatilityTracker {
    /// Creates a new tracker with the given configuration.
    pub fn new(config: VolatilityTrackerConfig) -> Self {
        Self { pools: HashMap::new(), config, last_block: None }
    }

    /// Updates all pools with current block's depths.
    /// Idempotent: skips if this block was already processed.
    pub fn update(&mut self, pool_depths: &HashMap<PoolDepthKey, BigUint>, block: u64) {
        if self
            .last_block
            .is_some_and(|lb| block <= lb)
        {
            return;
        }
        self.last_block = Some(block);

        let max_window = self.config.max_window();
        let alpha = self.config.alpha;

        for (key, depth_big) in pool_depths {
            let depth = depth_big.to_f64().unwrap_or(0.0);
            if depth <= 0.0 {
                continue;
            }

            let entry = self
                .pools
                .entry(key.clone())
                .or_insert_with(PoolVolatility::new);

            if let Some(prev) = entry.prev_depth {
                if prev > 0.0 {
                    let relative_delta = (depth - prev) / prev;
                    entry.push_delta(relative_delta, alpha, max_window);
                }
            }
            entry.prev_depth = Some(depth);
        }
    }

    /// Returns (σ_F, variance) for a pool if enough samples have been collected.
    pub fn get_volatility(&self, key: &PoolDepthKey) -> Option<(f64, f64)> {
        let min_samples = self.config.min_samples();
        self.pools.get(key).and_then(|vol| {
            if vol.sample_count >= min_samples {
                Some((vol.cached_sigma, vol.cached_variance))
            } else {
                None
            }
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn key(pool: &str) -> PoolDepthKey {
        (pool.to_string(), Default::default(), Default::default())
    }

    fn depths(entries: &[(&str, u64)]) -> HashMap<PoolDepthKey, BigUint> {
        entries
            .iter()
            .map(|(pool, depth)| (key(pool), BigUint::from(*depth)))
            .collect()
    }

    fn config_fast() -> VolatilityTrackerConfig {
        VolatilityTrackerConfig {
            window_secs: 120,
            min_samples_secs: 24,
            alpha: 0.95,
            block_time_secs: 12,
        }
    }

    #[test]
    fn first_block_no_delta() {
        let mut tracker = VolatilityTracker::new(config_fast());
        tracker.update(&depths(&[("pool1", 5000)]), 100);
        assert!(tracker
            .get_volatility(&key("pool1"))
            .is_none());
    }

    #[test]
    fn cold_start_returns_none_until_min_samples() {
        let config = VolatilityTrackerConfig {
            window_secs: 120,
            min_samples_secs: 36, // 3 blocks at 12s
            alpha: 0.95,
            block_time_secs: 12,
        };
        let mut tracker = VolatilityTracker::new(config);
        // min_samples = 36/12 = 3 deltas needed
        tracker.update(&depths(&[("pool1", 5000)]), 100); // no delta (first observation)
        tracker.update(&depths(&[("pool1", 4800)]), 101); // 1 delta
        tracker.update(&depths(&[("pool1", 4950)]), 102); // 2 deltas
        assert!(tracker
            .get_volatility(&key("pool1"))
            .is_none());

        tracker.update(&depths(&[("pool1", 4900)]), 103); // 3 deltas
        assert!(tracker
            .get_volatility(&key("pool1"))
            .is_some());
    }

    #[test]
    fn idempotent_same_block() {
        let mut tracker = VolatilityTracker::new(config_fast());
        tracker.update(&depths(&[("pool1", 5000)]), 100);
        tracker.update(&depths(&[("pool1", 9999)]), 100); // same block, ignored
        tracker.update(&depths(&[("pool1", 5100)]), 101);

        // Delta should be (5100-5000)/5000 = 0.02, not based on 9999
        let (_sigma, _) = tracker
            .get_volatility(&key("pool1"))
            .unwrap_or((0.0, 0.0));
        // With only 1 delta, sigma = 0 (no spread). But we need min_samples=2.
        // Let's add one more.
        tracker.update(&depths(&[("pool1", 5200)]), 102);
        let (sigma, _) = tracker
            .get_volatility(&key("pool1"))
            .unwrap();
        // deltas: 0.02, ~0.0196. sigma should be small (consistent moves)
        assert!(sigma < 0.01, "sigma should be small for consistent moves: {sigma}");
    }

    #[test]
    fn volatile_pool_has_higher_sigma() {
        let mut tracker = VolatilityTracker::new(config_fast());

        // Stable pool: small moves
        // Volatile pool: large moves
        for block in 100..110u64 {
            let stable_depth = if block % 2 == 0 { 5000 } else { 4990 };
            let volatile_depth = if block % 2 == 0 { 5000 } else { 4000 };
            tracker
                .update(&depths(&[("stable", stable_depth), ("volatile", volatile_depth)]), block);
        }

        let (sigma_stable, _) = tracker
            .get_volatility(&key("stable"))
            .unwrap();
        let (sigma_volatile, _) = tracker
            .get_volatility(&key("volatile"))
            .unwrap();
        assert!(
            sigma_volatile > sigma_stable,
            "volatile pool should have higher sigma: {sigma_volatile} vs {sigma_stable}"
        );
    }

    #[test]
    fn ring_buffer_respects_max_window() {
        let config = VolatilityTrackerConfig {
            window_secs: 36, // 3 blocks
            min_samples_secs: 12,
            alpha: 0.95,
            block_time_secs: 12,
        };
        let mut tracker = VolatilityTracker::new(config);

        for block in 100..110u64 {
            tracker.update(&depths(&[("pool1", 5000 + block * 10)]), block);
        }

        // Internal ring buffer should have at most 3 entries
        let vol = tracker
            .pools
            .get(&key("pool1"))
            .unwrap();
        assert!(vol.deltas.len() <= 3, "ring buffer exceeded max_window: {}", vol.deltas.len());
    }

    #[test]
    fn zero_depth_pool_skipped() {
        let mut tracker = VolatilityTracker::new(config_fast());
        tracker.update(&depths(&[("pool1", 0)]), 100);
        assert!(!tracker
            .pools
            .contains_key(&key("pool1")));
    }
}
