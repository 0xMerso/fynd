//! Slippage prediction and route reliability scoring.

/// Constant slippage predictor (baseline).
pub mod constant;
/// Core trait and types for slippage prediction.
pub mod predictor;
/// Route-level reliability scoring.
pub mod reliability;

pub use constant::ConstantPredictor;
pub use predictor::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};
pub use reliability::{ReliabilityConfig, RouteStats, risk_adjusted_amount};
