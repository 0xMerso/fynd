//! Slippage prediction and route reliability scoring.

/// Core trait and types for slippage prediction.
pub mod predictor;
/// Concrete predictor implementations.
pub mod predictors;
/// Route-level reliability scoring.
pub mod reliability;

pub use predictor::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};
pub use predictors::{ConstantPredictor, LinearPredictor};
pub use reliability::{ReliabilityConfig, RouteStats, risk_adjusted_amount};
