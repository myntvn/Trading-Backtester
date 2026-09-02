use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{
    engine::EngineConfig,
    strategy::{BuyHold, EmaCross},
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
}

fn main() -> Result<()> {
    let args = Args::parse();

    let bars =
        data::load_csv(&args.csv).with_context(|| format!("loading {}", args.csv.display()))?;

    let cfg = EngineConfig {
        initial_cash: args.cash,
        fee_bps: args.fees,
    };

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
