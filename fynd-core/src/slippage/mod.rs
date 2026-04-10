//! Slippage prediction and route reliability scoring.

/// Core trait and types for slippage prediction.
pub mod predictor;
/// Concrete predictor implementations.
pub mod predictors;
/// Route-level reliability scoring.
pub mod reliability;
/// Live per-pool volatility tracking.
pub mod tracker;

pub use predictor::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};
pub use predictors::{ConstantPredictor, LinearPredictor, VolatilityPredictor};
pub use reliability::{risk_adjusted_amount, ReliabilityConfig, RouteStats};
pub use tracker::{VolatilityTracker, VolatilityTrackerConfig};
