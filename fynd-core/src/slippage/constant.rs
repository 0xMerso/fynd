use super::predictor::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};

/// Returns the same hardcoded slippage for every pool.
/// Useful as a baseline and for tests.
pub struct ConstantPredictor {
    expected_slippage: f64,
}

impl ConstantPredictor {
    /// Creates a new `ConstantPredictor` with the given slippage value.
    pub fn new(expected_slippage: f64) -> Self {
        Self { expected_slippage }
    }
}

impl SlippagePredictor for ConstantPredictor {
    fn predict(&self, _features: &PoolSlippageFeatures) -> SlippagePrediction {
        SlippagePrediction { expected_slippage: self.expected_slippage, variance: None }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn returns_configured_slippage_for_any_input() {
        let predictor = ConstantPredictor::new(0.05);

        let features_low = PoolSlippageFeatures { utilization: 0.1, fee: 0.003 };
        let features_high = PoolSlippageFeatures { utilization: 0.9, fee: 0.01 };

        let pred_low = predictor.predict(&features_low);
        let pred_high = predictor.predict(&features_high);

        assert_eq!(pred_low.expected_slippage, 0.05);
        assert_eq!(pred_high.expected_slippage, 0.05);
        assert!(pred_low.variance.is_none());
        assert!(pred_high.variance.is_none());
    }
}
