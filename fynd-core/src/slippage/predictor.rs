use crate::derived::types::PoolDepths;

/// Per-pool signals available from `SharedMarketData` and `DerivedData`
/// that could predict next-block slippage.
#[derive(Debug, Clone)]
pub struct PoolSlippageFeatures {
    /// amount_in / pool_depth, clamped [0, 1].
    /// How much of the pool's safe capacity this swap consumes.
    pub utilization: f64,
    /// Pool fee from `ProtocolSim::fee()`, e.g. 0.003 for 0.3%.
    pub fee: f64,
    /// Pool identifier for volatility lookups. None for predictors that don't need it.
    pub pool_key: Option<crate::derived::types::PoolDepthKey>,
    /// Raw pool depth as f64. None when depth is unavailable.
    pub depth: Option<f64>,
}

/// Slippage prediction for a single pool.
#[derive(Debug, Clone)]
pub struct SlippagePrediction {
    /// Expected slippage as a fraction in [0, 1]. E.g. 0.03 = 3%.
    pub expected_slippage: f64,
    /// Optional variance of the slippage estimate.
    /// `None` for simple models (e.g. `ConstantPredictor`).
    pub variance: Option<f64>,
}

/// Predicts next-block slippage for a pool based on observable features.
/// Adding a new predictor requires zero changes to routing code.
pub trait SlippagePredictor: Send + Sync {
    /// Returns a slippage prediction for a single pool.
    fn predict(&self, features: &PoolSlippageFeatures) -> SlippagePrediction;

    /// Ingests new pool depth data for a block. Default no-op for static predictors.
    fn update(&self, _pool_depths: &PoolDepths, _block: u64) {}
}
