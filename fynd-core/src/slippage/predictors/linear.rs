//! Linear slippage predictor.
//!
//! Computes expected slippage as a weighted sum of pool features.
//! Weights are hardcoded — not trained, just plausible values.

use crate::slippage::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};

/// Predicts slippage as a weighted sum of pool features.
pub struct LinearPredictor {
    /// Weight for utilization (amount_in / pool_depth).
    w_utilization: f64,
    /// Weight for fee signal (inverted: low fee = higher slippage risk).
    w_fee: f64,
}

impl LinearPredictor {
    /// Creates a `LinearPredictor` with custom weights.
    pub fn new(w_utilization: f64, w_fee: f64) -> Self {
        Self { w_utilization, w_fee }
    }

    /// Creates a `LinearPredictor` with default plausible weights.
    pub fn default_weights() -> Self {
        Self { w_utilization: 0.8, w_fee: 0.2 }
    }
}

impl SlippagePredictor for LinearPredictor {
    fn predict(&self, features: &PoolSlippageFeatures) -> SlippagePrediction {
        // We assume higher utilization = more slippage risk (direct)
        // Lower fee = more arb activity = less stable (inverted, normalized to 1% = 1.0)
        //
        let fee_signal = 1.0 - (features.fee / 0.01).min(1.0);

        let expected =
            (self.w_utilization * features.utilization + self.w_fee * fee_signal).clamp(0.0, 1.0);

        // Higher expected slippage = higher uncertainty
        let variance = expected;

        SlippagePrediction { expected_slippage: expected, variance: Some(variance) }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn low_utilization_low_slippage() {
        let predictor = LinearPredictor::default_weights();
        let features = PoolSlippageFeatures { utilization: 0.1, fee: 0.003 };
        let prediction = predictor.predict(&features);

        assert!(prediction.expected_slippage < 0.3);
        assert!(prediction.variance.is_some());
    }

    #[test]
    fn high_utilization_high_slippage() {
        let predictor = LinearPredictor::default_weights();
        let features = PoolSlippageFeatures { utilization: 0.9, fee: 0.003 };
        let prediction = predictor.predict(&features);

        assert!(prediction.expected_slippage > 0.5);
    }

    #[test]
    fn high_fee_reduces_slippage() {
        let predictor = LinearPredictor::default_weights();
        let low_fee = PoolSlippageFeatures { utilization: 0.5, fee: 0.001 };
        let high_fee = PoolSlippageFeatures { utilization: 0.5, fee: 0.01 };

        let pred_low = predictor.predict(&low_fee);
        let pred_high = predictor.predict(&high_fee);

        assert!(pred_low.expected_slippage > pred_high.expected_slippage);
    }

    #[test]
    fn output_clamped_to_unit_range() {
        let predictor = LinearPredictor::new(10.0, 10.0);
        let features = PoolSlippageFeatures { utilization: 1.0, fee: 0.0 };
        let prediction = predictor.predict(&features);

        assert!(prediction.expected_slippage <= 1.0);
        assert!(prediction.expected_slippage >= 0.0);
    }
}
