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

impl RouteStats {
    /// Creates an empty `RouteStats`.
    pub fn new() -> Self {
        Self { expected_delivery: 1.0, sum_variance: 0.0, pool_count: 0 }
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
