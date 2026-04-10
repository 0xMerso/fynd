//! Volatility-based slippage predictor using live block-to-block depth deltas.

use std::sync::Arc;

use tokio::sync::RwLock;

use crate::{
    derived::types::PoolDepths,
    slippage::{
        predictors::linear::LinearPredictor, tracker::VolatilityTracker, PoolSlippageFeatures,
        SlippagePrediction, SlippagePredictor,
    },
};

/// Predicts slippage from observed per-pool volatility (σ_F).
///
/// Falls back to `LinearPredictor` during cold start (not enough samples).
/// Formula: `expected_slippage = σ_F × utilization` (relative deltas, scale-independent).
pub struct VolatilityPredictor {
    tracker: Arc<RwLock<VolatilityTracker>>,
    fallback: LinearPredictor,
}

impl VolatilityPredictor {
    /// Creates a new `VolatilityPredictor` with default `LinearPredictor` fallback.
    pub fn new(tracker: Arc<RwLock<VolatilityTracker>>) -> Self {
        Self { tracker, fallback: LinearPredictor::default_weights() }
    }

    /// Creates a new `VolatilityPredictor` with a custom `LinearPredictor` fallback.
    pub fn with_fallback(
        tracker: Arc<RwLock<VolatilityTracker>>,
        fallback: LinearPredictor,
    ) -> Self {
        Self { tracker, fallback }
    }
}

impl SlippagePredictor for VolatilityPredictor {
    fn update(&self, pool_depths: &PoolDepths, block: u64) {
        if let Ok(mut guard) = self.tracker.try_write() {
            guard.update(pool_depths, block);
        }
    }

    fn predict(&self, features: &PoolSlippageFeatures) -> SlippagePrediction {
        let Some(pool_key) = &features.pool_key else {
            return self.fallback.predict(features);
        };

        // try_read to avoid blocking the hot path; fall back if locked
        let guard = match self.tracker.try_read() {
            Ok(g) => g,
            Err(_) => return self.fallback.predict(features),
        };

        match guard.get_volatility(pool_key) {
            Some((sigma_f, variance)) => {
                let expected = (sigma_f * features.utilization).clamp(0.0, 1.0);
                SlippagePrediction { expected_slippage: expected, variance: Some(variance) }
            }
            None => self.fallback.predict(features),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use num_bigint::BigUint;

    use super::*;
    use crate::slippage::tracker::VolatilityTrackerConfig;

    fn key(pool: &str) -> crate::derived::types::PoolDepthKey {
        (pool.to_string(), Default::default(), Default::default())
    }

    fn depths(entries: &[(&str, u64)]) -> HashMap<crate::derived::types::PoolDepthKey, BigUint> {
        entries
            .iter()
            .map(|(pool, depth)| (key(pool), BigUint::from(*depth)))
            .collect()
    }

    fn config_fast() -> VolatilityTrackerConfig {
        VolatilityTrackerConfig {
            window_secs: 120,
            min_samples_secs: 24, // 2 blocks at 12s
            alpha: 0.95,
            block_time_secs: 12,
        }
    }

    fn build_tracker_with_data() -> Arc<RwLock<VolatilityTracker>> {
        let mut tracker = VolatilityTracker::new(config_fast());
        // Simulate blocks with varying depths
        tracker.update(&depths(&[("pool1", 5000)]), 100);
        tracker.update(&depths(&[("pool1", 4800)]), 101); // -4%
        tracker.update(&depths(&[("pool1", 5100)]), 102); // +6.25%
        tracker.update(&depths(&[("pool1", 4900)]), 103); // -3.9%
        Arc::new(RwLock::new(tracker))
    }

    #[test]
    fn predicts_from_volatility_when_data_available() {
        let tracker = build_tracker_with_data();
        let predictor = VolatilityPredictor::new(tracker);

        let features = PoolSlippageFeatures {
            utilization: 0.5,
            fee: 0.003,
            pool_key: Some(key("pool1")),
            depth: Some(5000.0),
        };
        let prediction = predictor.predict(&features);

        // σ_F should be non-trivial for these swings
        assert!(prediction.expected_slippage > 0.0);
        assert!(prediction.expected_slippage < 1.0);
        assert!(prediction.variance.is_some());
    }

    #[test]
    fn falls_back_to_linear_when_no_data() {
        let tracker = Arc::new(RwLock::new(VolatilityTracker::new(config_fast())));
        let predictor = VolatilityPredictor::new(tracker);

        let features = PoolSlippageFeatures {
            utilization: 0.5,
            fee: 0.003,
            pool_key: Some(key("unknown_pool")),
            depth: Some(5000.0),
        };
        let prediction = predictor.predict(&features);

        // Should get LinearPredictor's result
        let linear = LinearPredictor::default_weights();
        let linear_prediction = linear.predict(&features);
        assert!((prediction.expected_slippage - linear_prediction.expected_slippage).abs() < 1e-10);
    }

    #[test]
    fn falls_back_when_no_pool_key() {
        let tracker = build_tracker_with_data();
        let predictor = VolatilityPredictor::new(tracker);

        let features =
            PoolSlippageFeatures { utilization: 0.5, fee: 0.003, pool_key: None, depth: None };
        let prediction = predictor.predict(&features);

        let linear = LinearPredictor::default_weights();
        let linear_prediction = linear.predict(&features);
        assert!((prediction.expected_slippage - linear_prediction.expected_slippage).abs() < 1e-10);
    }

    #[test]
    fn higher_utilization_means_higher_slippage() {
        let tracker = build_tracker_with_data();
        let predictor = VolatilityPredictor::new(tracker);

        let low_util = PoolSlippageFeatures {
            utilization: 0.1,
            fee: 0.003,
            pool_key: Some(key("pool1")),
            depth: Some(5000.0),
        };
        let high_util = PoolSlippageFeatures {
            utilization: 0.9,
            fee: 0.003,
            pool_key: Some(key("pool1")),
            depth: Some(5000.0),
        };

        let pred_low = predictor.predict(&low_util);
        let pred_high = predictor.predict(&high_util);

        assert!(
            pred_high.expected_slippage > pred_low.expected_slippage,
            "higher utilization should mean higher slippage: {} vs {}",
            pred_high.expected_slippage,
            pred_low.expected_slippage
        );
    }

    #[test]
    fn volatile_pool_penalized_more_than_stable() {
        let mut tracker = VolatilityTracker::new(config_fast());

        // Stable pool: tiny moves
        // Volatile pool: big swings
        for block in 100..110u64 {
            let stable = if block % 2 == 0 { 5000 } else { 4995 };
            let volatile = if block % 2 == 0 { 5000 } else { 4000 };
            tracker.update(&depths(&[("stable", stable), ("volatile", volatile)]), block);
        }

        let tracker = Arc::new(RwLock::new(tracker));
        let predictor = VolatilityPredictor::new(tracker);

        let stable_features = PoolSlippageFeatures {
            utilization: 0.5,
            fee: 0.003,
            pool_key: Some(key("stable")),
            depth: Some(5000.0),
        };
        let volatile_features = PoolSlippageFeatures {
            utilization: 0.5,
            fee: 0.003,
            pool_key: Some(key("volatile")),
            depth: Some(5000.0),
        };

        let pred_stable = predictor.predict(&stable_features);
        let pred_volatile = predictor.predict(&volatile_features);

        assert!(
            pred_volatile.expected_slippage > pred_stable.expected_slippage,
            "volatile pool should have higher slippage: {} vs {}",
            pred_volatile.expected_slippage,
            pred_stable.expected_slippage
        );
    }
}
