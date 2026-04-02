//! Route-level reliability scoring.

use super::predictor::SlippagePrediction;

/// Accumulates per-pool slippage predictions into a route-level summary.
#[derive(Debug, Clone)]
pub struct RouteStats {
    /// Product of (1 - expected_slippage) across all pools.
    expected_delivery: f64,
    /// Sum of per-pool variances.
    sum_variance: f64,
    /// Number of pools added.
    pool_count: usize,
}

impl Default for RouteStats {
    fn default() -> Self {
        Self { expected_delivery: 1.0, sum_variance: 0.0, pool_count: 0 }
    }
}

impl RouteStats {
    /// Creates an empty `RouteStats`.
    pub fn new() -> Self {
        Self::default()
    }

    /// Adds a pool's prediction to the route summary.
    pub fn add_pool(&mut self, prediction: &SlippagePrediction) {
        self.expected_delivery *= 1.0 - prediction.expected_slippage;
        self.sum_variance += prediction.variance.unwrap_or(0.0);
        self.pool_count += 1;
    }

    /// Expected fraction of quoted output that will be delivered.
    pub fn expected_delivery(&self) -> f64 {
        self.expected_delivery
    }

    /// Route-level standard deviation (sqrt of summed variances).
    pub fn route_std_dev(&self) -> f64 {
        self.sum_variance.sqrt()
    }

    /// Number of pools in this route.
    pub fn pool_count(&self) -> usize {
        self.pool_count
    }
}

/// Tunable parameters for route reliability scoring.
#[derive(Debug, Clone)]
pub struct ReliabilityConfig {
    /// Risk aversion coefficient. 0.0 = ignore variance (same ranking as today).
    /// Higher = prefer reliable routes over optimistic ones.
    pub lambda: f64,
}

impl Default for ReliabilityConfig {
    fn default() -> Self {
        Self { lambda: 0.0 }
    }
}

/// Produces a single scalar for route comparison.
///
/// `amount_out` and `gas_cost` are in the same token units.
/// When `lambda = 0`, ranking is determined only by expected delivery.
/// When `lambda > 0`, high-variance routes are penalized.
pub fn risk_adjusted_amount(
    amount_out: f64,
    stats: &RouteStats,
    gas_cost: f64,
    config: &ReliabilityConfig,
) -> f64 {
    let net = amount_out - gas_cost;
    net * stats.expected_delivery() - config.lambda * net * stats.route_std_dev()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prediction(expected_slippage: f64, variance: Option<f64>) -> SlippagePrediction {
        SlippagePrediction { expected_slippage, variance }
    }

    // Spec test 2: RouteStats accumulates correctly across multiple pools.
    #[test]
    fn route_stats_accumulates_across_pools() {
        let mut stats = RouteStats::new();
        stats.add_pool(&prediction(0.10, Some(1.0)));
        stats.add_pool(&prediction(0.20, Some(3.0)));

        assert_eq!(stats.pool_count(), 2);
        // expected_delivery = (1 - 0.10) * (1 - 0.20) = 0.9 * 0.8 = 0.72
        assert!((stats.expected_delivery() - 0.72).abs() < 1e-10);
        // route_std_dev = sqrt(1.0 + 3.0) = sqrt(4.0) = 2.0
        assert!((stats.route_std_dev() - 2.0).abs() < 1e-10);
    }

    // Spec test 3: risk_adjusted_amount at lambda=0.0 returns same ranking as raw output.
    #[test]
    fn lambda_zero_preserves_ranking() {
        let config = ReliabilityConfig { lambda: 0.0 };

        // Route A: high output, high slippage
        let mut stats_a = RouteStats::new();
        stats_a.add_pool(&prediction(0.10, Some(0.10)));
        let score_a = risk_adjusted_amount(1000.0, &stats_a, 0.0, &config);

        // Route B: lower output, lower slippage
        let mut stats_b = RouteStats::new();
        stats_b.add_pool(&prediction(0.02, Some(0.02)));
        let score_b = risk_adjusted_amount(950.0, &stats_b, 0.0, &config);

        // At lambda=0, score = net * expected_delivery. No variance penalty.
        // A: 1000 * 0.90 = 900, B: 950 * 0.98 = 931. B wins.
        assert!(score_b > score_a);

        // Verify same ranking as comparing raw * expected_delivery directly
        let raw_a = 1000.0 * stats_a.expected_delivery();
        let raw_b = 950.0 * stats_b.expected_delivery();
        assert_eq!(score_a > score_b, raw_a > raw_b);
    }

    // Spec test 4: risk_adjusted_amount at lambda > 0.0 selects the lower-variance
    // route when it has slightly lower gross output.
    #[test]
    fn lambda_positive_selects_lower_variance() {
        let config = ReliabilityConfig { lambda: 1.0 };

        // Route A: slightly higher output, high variance
        let mut stats_a = RouteStats::new();
        stats_a.add_pool(&prediction(0.05, Some(0.50)));
        let score_a = risk_adjusted_amount(1000.0, &stats_a, 0.0, &config);

        // Route B: slightly lower output, low variance
        let mut stats_b = RouteStats::new();
        stats_b.add_pool(&prediction(0.05, Some(0.01)));
        let score_b = risk_adjusted_amount(995.0, &stats_b, 0.0, &config);

        // Same expected slippage, but A has much higher variance.
        // Lambda penalizes A's variance, so B should win despite lower raw output.
        assert!(score_b > score_a);
    }
}
