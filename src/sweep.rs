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

#[cfg(test)]
mod tests {
    use crate::{
        data::Bar,
        engine::EngineConfig,
        sweep::{SweepConfig, rank_by_train_sharpe, rank_key, split_at, sweep},
    };

    /// A wobby uptrend, so EMAs actually cross.
    fn bars(n: usize) -> Vec<Bar> {
        (0..n)
            .map(|i| {
                let c = 100.0 + i as f64 + (i as f64 * 0.7).sin() * 8.0;
                Bar {
                    ts: i as i64 * 86_400_000,
                    open: c,
                    high: c,
                    low: c,
                    close: c,
                    volume: 0.0,
                }
            })
            .collect()
    }

    fn cfg() -> EngineConfig {
        EngineConfig {
            initial_cash: 10_000.0,
            fee_bps: 10.0,
        }
    }

    fn sc<'a>(fast: &'a [usize], slow: &'a [usize]) -> SweepConfig<'a> {
        SweepConfig {
            split: 0.7,
            fast_range: fast,
            slow_range: slow,
            periods_per_year: 365.0,
        }
    }

    #[test]
    fn one_result_per_valid_combination() {
        let b = bars(300);
        let r = sweep(&b, &cfg(), &sc(&[5, 10], &[20, 30])).unwrap();
        assert_eq!(r.len(), 4);
    }

    #[test]
    fn invalid_combinations_are_skipped() {
        let b = bars(300);
        // 10/20 is valid; 30/20 is not (fast >= slow).
        let r = sweep(&b, &cfg(), &sc(&[10, 30], &[20])).unwrap();

        assert_eq!(r.len(), 1);
        assert_eq!((r[0].fast, r[0].slow), (10, 20));
    }

    #[test]
    fn all_invalid_is_an_error() {
        let b = bars(300);
        assert!(sweep(&b, &cfg(), &sc(&[50], &[20])).is_err());
    }

    #[test]
    fn is_deterministic_across_runs() {
        let b = bars(400);
        let a = sweep(&b, &cfg(), &sc(&[5, 10, 15], &[30, 50, 80])).unwrap();
        let c = sweep(&b, &cfg(), &sc(&[5, 10, 15], &[30, 50, 80])).unwrap();

        assert_eq!(a.len(), c.len());

        for (x, y) in a.iter().zip(c.iter()) {
            assert_eq!((x.fast, x.slow), (y.fast, y.slow));
            // to_bits() is exact bitwise equality — stronger than ==, and
            // the right assertion for "the same computation ran twice".
            assert_eq!(
                x.train.final_equity.to_bits(),
                y.train.final_equity.to_bits()
            );
            assert_eq!(x.test.final_equity.to_bits(), y.test.final_equity.to_bits());
        }
    }

    #[test]
    fn split_index_is_a_fraction_of_the_bars() {
        assert_eq!(split_at(100, 0.7), 70);
        assert_eq!(split_at(1827, 0.7), 1278);
    }

    #[test]
    fn train_and_test_do_not_overlap() {
        let b = bars(100);
        let k = split_at(b.len(), 0.7);
        let (train, test) = b.split_at(k);

        assert_eq!(train.len(), 70);
        assert_eq!(test.len(), 30);
        assert!(train.last().unwrap().ts < test.first().unwrap().ts);
    }

    #[test]
    fn rejects_degenerate_splits() {
        let b = bars(300);
        let fast = [10usize];
        let slow = [30usize];

        for bad in [0.0, 1.0, -0.5, 1.5] {
            let s = SweepConfig {
                split: bad,
                fast_range: &fast,
                slow_range: &slow,
                periods_per_year: 365.0,
            };
            assert!(
                sweep(&b, &cfg(), &s).is_err(),
                "split {bad} should be rejected"
            );
        }
    }

    #[test]
    fn rejects_a_split_that_leaves_too_few_bars() {
        let b = bars(3);
        let fast = [10usize];
        let slow = [30usize];
        let s = SweepConfig {
            split: 0.5,
            fast_range: &fast,
            slow_range: &slow,
            periods_per_year: 365.0,
        };

        assert!(sweep(&b, &cfg(), &s).is_err());
    }

    #[test]
    fn a_configuration_that_never_trades_has_no_sharpe() {
        // slow = 290 never warms up on a 210-bar training slice.
        let b = bars(300);
        let r = sweep(&b, &cfg(), &sc(&[5], &[290])).unwrap();

        assert_eq!(r[0].train.trades, 0);
        assert_eq!(r[0].train.sharpe, None);
        assert_eq!(rank_key(&r[0].train), f64::NEG_INFINITY);
    }

    #[test]
    fn ranking_is_descending_with_none_last() {
        let mut keys = [0.5, f64::NEG_INFINITY, -2.0];
        keys.sort_by(|a, b| b.total_cmp(a));

        assert_eq!(keys, [0.5, -2.0, f64::NEG_INFINITY]);
    }

    #[test]
    fn ranking_orders_results_by_train_sharpe() {
        let b = bars(400);
        let mut r = sweep(&b, &cfg(), &sc(&[5, 10, 15], &[30, 50, 80])).unwrap();
        rank_by_train_sharpe(&mut r);

        let keys: Vec<f64> = r.iter().map(|x| rank_key(&x.train)).collect();
        for w in keys.windows(2) {
            assert!(w[0] >= w[1], "not sorted descending: {keys:?}");
        }
    }
}
