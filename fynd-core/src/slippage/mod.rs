//! Slippage prediction and route reliability scoring.

/// Constant slippage predictor (baseline).
pub mod constant;
/// Core trait and types for slippage prediction.
pub mod predictor;

pub use constant::ConstantPredictor;
pub use predictor::{PoolSlippageFeatures, SlippagePrediction, SlippagePredictor};
