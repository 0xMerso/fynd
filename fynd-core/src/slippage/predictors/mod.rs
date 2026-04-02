//! Concrete `SlippagePredictor` implementations.

/// Constant slippage predictor (baseline).
pub mod constant;
/// Linear weighted predictor.
pub mod linear;

pub use constant::ConstantPredictor;
pub use linear::LinearPredictor;
