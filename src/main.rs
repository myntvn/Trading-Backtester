use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{
    data::Bar,
    engine::EngineConfig,
    strategy::{BuyHold, EmaCross},
    sweep::SweepConfig,
};

use clap::Parser;

mod data;
mod engine;
mod indicators;
mod metrics;
mod report;
mod strategy;
mod sweep;

#[derive(Parser, Debug)]
#[command(version, about = "EMA crossover backtester for OHLCV data")]
struct Args {
    /// Path to a Binance-format kline CSV
    csv: PathBuf,

    /// Fast EMA period
    #[arg(long, default_value_t = 20)]
    fast: usize,

    /// Slow EMA period
    #[arg(long, default_value_t = 50)]
    slow: usize,

    /// Starting capital
    #[arg(long, default_value_t = 10_000.0)]
    cash: f64,

    /// Per-side fee in basis points (1 bp = 0.01%)
    #[arg(long, default_value_t = 10.0)]
    fees: f64,

    /// Bars per year for Sharpe annualisation (365 daily crypto, 252 equities)
    #[arg(long, default_value_t = 365.0)]
    periods_per_year: f64,

    /// Skip the buy & hold comparison column
    #[arg(long)]
    no_benchmark: bool,

    /// Run a parameter sweep instead of a single backtest
    #[arg(long)]
    sweep: bool,

    /// Fast EMA periods to try when sweeping
    #[arg(long, value_delimiter = ',', default_value = "5,8,10,12,15,20,25,30")]
    fast_range: Vec<usize>,

    /// Slow EMA periods to try when sweeping
    #[arg(
        long,
        value_delimiter = ',',
        default_value = "20,30,40,50,80,100,150,200"
    )]
    slow_range: Vec<usize>,

    /// Fraction of bars used for training
    #[arg(long, default_value_t = 0.7)]
    split: f64,

    /// How many sweep results to show
    #[arg(long, default_value_t = 15)]
    top: usize,
}

fn main() -> Result<()> {
    let args = Args::parse();

    let bars =
        data::load_csv(&args.csv).with_context(|| format!("loading {}", args.csv.display()))?;

    let cfg = EngineConfig {
        initial_cash: args.cash,
        fee_bps: args.fees,
    };

    if args.sweep {
        return run_sweep(&bars, &cfg, &args);
    }

    let strat = EmaCross::new(args.fast, args.slow)?;
    let bt = engine::run(&bars, &strat, &cfg)?;
    let m = metrics::compute(&bt, args.periods_per_year);

    report::print_header(bars.len(), bars[0].ts, bars[bars.len() - 1].ts, cfg.fee_bps);
    report::print_trades(&bt.trades);

    if args.no_benchmark {
        report::print_comparison(&[(bt.strategy.as_str(), &m)]);
    } else {
        let bench = engine::run(&bars, &BuyHold, &cfg)?;
        let bm = metrics::compute(&bench, args.periods_per_year);

        report::print_comparison(&[(bt.strategy.as_str(), &m), (bench.strategy.as_str(), &bm)]);
    }

    Ok(())
}

fn run_sweep(bars: &[Bar], cfg: &EngineConfig, args: &Args) -> Result<()> {
    let sc = SweepConfig {
        split: args.split,
        fast_range: &args.fast_range,
        slow_range: &args.slow_range,
        periods_per_year: args.periods_per_year,
    };

    let mut results = sweep::sweep(bars, cfg, &sc)?;
    let combos = results.len();
    sweep::rank_by_train_sharpe(&mut results);

    let k = sweep::split_at(bars.len(), args.split);

    report::print_header(bars.len(), bars[0].ts, bars[bars.len() - 1].ts, cfg.fee_bps);
    report::print_split(bars, k, args.split);
    report::print_sweep(&results, args.top);

    // The benchmark must cover the same test window as the strategy.
    let bench_bt = engine::run(&bars[k..], &strategy::BuyHold, cfg)?;
    let bench = metrics::compute(&bench_bt, args.periods_per_year);

    report::print_verdict(&results[0], &bench, combos);

    Ok(())
}

#[cfg(test)]
mod tests {
    use clap::{CommandFactory, Parser};

    use crate::Args;

    #[test]
    fn cli_is_well_formed() {
        Args::command().debug_assert();
    }

    #[test]
    fn defaults_are_applied() {
        let a = Args::try_parse_from(["backtester", "x.csv"]).unwrap();

        assert_eq!(a.fast, 20);
        assert_eq!(a.slow, 50);
        assert_eq!(a.cash, 10_000.0);
        assert_eq!(a.periods_per_year, 365.0);
        assert!(!a.no_benchmark);
    }

    #[test]
    fn flags_override_defaults() {
        let a = Args::try_parse_from([
            "backtester",
            "x.csv",
            "--fast",
            "5",
            "--slow",
            "20",
            "--no-benchmark",
        ])
        .unwrap();

        assert_eq!(a.fast, 5);
        assert_eq!(a.slow, 20);
        assert!(a.no_benchmark);
    }

    #[test]
    fn csv_argument_is_required() {
        assert!(Args::try_parse_from(["backtester"]).is_err());
    }
}
