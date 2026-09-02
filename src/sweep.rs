use anyhow::{Result, ensure};
use rayon::iter::{IntoParallelRefIterator, ParallelIterator};

use crate::{
    data::Bar,
    engine::{self, EngineConfig},
    metrics::{self, Metrics},
    strategy::EmaCross,
};

#[derive(Debug, Clone)]
pub struct SweepResult {
    pub fast: usize,
    pub slow: usize,
    pub train: Metrics,
    pub test: Metrics,
}

pub struct SweepConfig<'a> {
    /// Fraction of bars used for training.
    pub split: f64,
    pub fast_range: &'a [usize],
    pub slow_range: &'a [usize],
    pub periods_per_year: f64,
}

/// Index where the training slice ends and the test slice begins.
pub fn split_at(n: usize, split: f64) -> usize {
    (n as f64 * split) as usize
}

/// Sharpe with `None` mapped to the worst possible value, so a configuration
/// that never traded ranks below one that traded badly.
pub fn rank_key(m: &Metrics) -> f64 {
    m.sharpe.unwrap_or(f64::NEG_INFINITY)
}

/// Sort descending by training Sharpe.
///
/// `total_cmp` gives a total ordering over all `f64` values including `NaN`,
/// so this cannot panic. `partial_cmp(..).unwrap()` would.
pub fn rank_by_train_sharpe(results: &mut [SweepResult]) {
    results.sort_by(|a, b| rank_key(&b.train).total_cmp(&rank_key(&a.train)));
}

pub fn sweep(bars: &[Bar], cfg: &EngineConfig, sc: &SweepConfig) -> Result<Vec<SweepResult>> {
    ensure!(
        sc.split > 0.0 && sc.split < 1.0,
        "split must be between 0 and 1 (got {})",
        sc.split
    );

    ensure!(!sc.fast_range.is_empty(), "fast range is empty");
    ensure!(!sc.slow_range.is_empty(), "slow range is empty");

    let k = split_at(bars.len(), sc.split);

    ensure!(
        k >= 2 && bars.len().saturating_sub(k) >= 2,
        "split leaves too few bars: {} train / {} test",
        k,
        bars.len().saturating_sub(k)
    );

    // Slice the BARS, not the equity curve. Running once on the full series
    // and slicing the result would let the test period inherit a position
    // opened on training data.
    let (train_bars, test_bars) = bars.split_at(k);

    let combos: Vec<(usize, usize)> = sc
        .fast_range
        .iter()
        .flat_map(|&f| sc.slow_range.iter().map(move |&s| (f, s)))
        .collect();

    // `.par_iter()` is the only difference from a sequential sweep. It works
    // with no locks because `engine::run` takes `&[Bar]` and returns owned
    // data — no shared mutable state anywhere.
    let results: Vec<SweepResult> = combos
        .par_iter()
        .filter_map(|&(fast, slow)| {
            let strat = EmaCross::new(fast, slow).ok()?;

            let train_bt = engine::run(train_bars, &strat, cfg).ok()?;
            let test_bt = engine::run(test_bars, &strat, cfg).ok()?;

            Some(SweepResult {
                fast,
                slow,
                train: metrics::compute(&train_bt, sc.periods_per_year),
                test: metrics::compute(&test_bt, sc.periods_per_year),
            })
        })
        .collect();

    ensure!(
        !results.is_empty(),
        "no valid (fast, slow) combinations in the given ranges"
    );
    Ok(results)
}
