# SOLUTION.md

---

## Part 1 — Design

### 1. Slippage prediction interface

- **trait** generic interface, routing code is decoupled from the model behind it:
  ```rust
  pub trait SlippagePredictor: Send + Sync {
      fn predict(&self, features: &PoolSlippageFeatures) -> SlippagePrediction;
  }
  ```
- **inputs** per-pool observable characteristics:
  ```rust
  pub struct PoolSlippageFeatures {
      pub utilization: f64,          // amount_in / pool_depth, clamped [0, 1]
      pub fee: f64,                  // ProtocolSim::fee(), e.g. 0.003 for 0.3%
  }
  ```
- **output** point estimate always present, variance optional for richer models:
  ```rust
  pub struct SlippagePrediction {
      pub expected_slippage: f64,    // [0, 1], e.g. 0.03 = 3%
      pub variance: Option<f64>,    // None for simple models
  }
  ```
- **two implementations**: `ConstantPredictor` (fixed slippage for any pool, baseline/tests) and `LinearPredictor` (weighted sum of features, different output per pool).

### 2. Feature extraction

- `utilization` = `amount_in / pool_depth`, strongest signal. deeper pools are more stable, volatility scales with `1/sqrt(pool_depth)`.
- `fee` from `ProtocolSim::fee()`, higher fee pools get arbitraged less often, reserves move less between blocks.
- `protocol_confidence` dropped (arbitrary without data). two proper paths: derive from AMM curve convexity, or measure empirically.
- ignored for now: spot_price, token_quality, gas_price.

### 3. Label creation

- save quoted output at block N, re-simulate at block N+1, delta = label.
- limitations: selection bias (only quoted routes), re-simulation ≠ on-chain execution (MEV, tx ordering).
- over time labels accumulate into per-pool volatility history — exactly what a `HistoricalPredictor` would consume.

### 4. Integration point

- `MostLiquidAlgorithm::find_best_route` simulation selection loop.
- per quote, per candidate route. pure f64 arithmetic, no locks, O(hops).
- `None` predictor = identical behavior to today.

### 5. Scoring with uncertainty

- `score = net_amount_out × expected_delivery - lambda × net_amount_out × route_std_dev`
- `expected_delivery` = fraction of quoted output actually delivered, from aggregated pool predictions.
- `route_std_dev` = sqrt of summed per-pool variances. `variance: None` contributes 0, lambda has no effect.
- `lambda` = configurable risk aversion. 0 = same ranking as today.

---

## Part 2 — Implementation notes

### Timeline (~8h, mostly codebase review, 2-3h implementing)

1. read specs, cloned, reviewed codebase beyond what was required (encoding, feed, worker pools)
2. tried quickstart/examples. slept on it, thought about approaches without AI at first
3. quickly reviewed how existing dex aggregators handle dynamic slippage
4. looked at volatility pricing models for theoretical grounding
5. discussed static vs dynamic heuristic with Claude. went static (no historical data available). had a lot of ideas but ended up applying the tech test specs scrupulously
6. mapped SharedMarketData + DerivedData fields. kept utilization + fee, dropped the rest
7. implemented: trait, ConstantPredictor, LinearPredictor, reliability scoring, hook into MostLiquidAlgorithm
8. all 5 spec tests pass, clippy clean, full workspace green

### AI usage

used Cursor and Claude Code. Claude wrote tests and implementation scaffolding following existing codebase patterns. I pushed back on:
- removed `protocol_confidence` (AI suggested hardcoded values, I found them arbitrary)
- simplified variance formula (AI proposed beta-distribution-like curve, I said just `variance = expected`)
- challenged fee as a signal (kept it as best available, noted weakness)
- restructured folder layout myself to match derived/computations/ pattern

what I did without AI: initial design thinking, options pricing analogy, decision to drop features, understanding the complementarity between try_score_path and score_route.

### Key decisions

- **dropped protocol_confidence**: no empirical basis. two proper paths: derive from AMM curve convexity, or measure historically
- **variance = expected_slippage**: simple, no made-up formula
- **gas_cost = 0.0 in score_route**: gas already subtracted in net_amount_out
- **predictors/ subfolder**: follows derived/computations/ pattern

### Observation

real-time single-block assessment for dynamically evaluating slippage is insufficient — there's room for small optimizations like the ones done here (utilization, fee weighting), but meaningful slippage prediction only makes sense with a historical dataset. per-pool volatility built over hundreds of blocks is the real signal, not a snapshot heuristic.

### What I'd continue with

happy to extend. two directions: a **HistoricalPredictor** that accumulates per-pool volatility over blocks (Tycho already streams ProtocolStateDelta, Fynd just discards them), and **richer features** via a listener tracking reserve deltas, volume/swap count per pool, gas trends — the real slippage signals.

couldn't work the full day on it but available Friday and the weekend for any extension. glad to dig into Fynd, codebase is well structured.
