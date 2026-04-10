//! Concrete `SlippagePredictor` implementations.

/// Constant slippage predictor (baseline).
pub mod constant;
/// Linear weighted predictor.
pub mod linear;
/// Volatility-based predictor using live block-to-block depth deltas.
pub mod volatility;

pub use constant::ConstantPredictor;
pub use linear::LinearPredictor;
pub use volatility::VolatilityPredictor;
