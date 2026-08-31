use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{data::fmt_date, engine::EngineConfig};

mod data;
mod engine;
mod indicators;
mod metrics;
mod strategy;

const FAST: usize = 20;
const SLOW: usize = 50;
/// Daily crypto bars: 365, not the 252 used for equities.
const PERIODS_PER_YEAR: f64 = 365.0;

const LABEL_W: usize = 16;
const COL_W: usize = 18;

fn fmt_opt(v: Option<f64>, suffix: &str) -> String {
    match v {
        Some(x) => format!("{x:.2}{suffix}"),
        None => "n/a".to_string(),
    }
}

fn row(label: &str, a: String, b: String) {
    println!("{label:<LABEL_W$}{a:>COL_W$}{b:>COL_W$}");
}

fn main() -> Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .context("usage: backtester <path-to-csv>")?
        .into();

    let bars = data::load_csv(&path)?;
    let strat = strategy::EmaCross::new(FAST, SLOW)?;
    let cfg = EngineConfig::default();

    let bt = engine::run(&bars, &strat, &cfg)?;
    let m = metrics::compute(&bt, PERIODS_PER_YEAR);

    let bench = engine::run(&bars, &strategy::BuyHold, &cfg)?;
    let bm = metrics::compute(&bench, PERIODS_PER_YEAR);

    println!(
        "{} bars   {} -> {}   {:.1} bps fees",
        bars.len(),
        fmt_date(bars[0].ts),
        fmt_date(bars[bars.len() - 1].ts),
        cfg.fee_bps,
    );

    println!("\ntrade history");

    println!(
        "{:<12} {:<12} {:>10} {:>10} {:>11} {:>8}",
        "entry", "exit", "in", "out", "pnl", "ret%"
    );
    for trade in &bt.trades {
        println!(
            "{:<12} {:<12} {:>10.2} {:>10.2} {:>11.2} {:>7.2}%{}",
            fmt_date(trade.entry_ts),
            fmt_date(trade.exit_ts),
            trade.entry_price,
            trade.exit_price,
            trade.pnl,
            trade.return_pct(),
            if trade.forced_exit {
                "  *still open"
            } else {
                ""
            },
        )
    }

    println!();
    println!(
        "{:<LABEL_W$}{:>COL_W$}{:>COL_W$}",
        "", bt.strategy, bench.strategy
    );
    println!("{}", "-".repeat(LABEL_W + 2 * COL_W));

    row(
        "final equity",
        format!("{:.2}", m.final_equity),
        format!("{:.2}", bm.final_equity),
    );
    row("pnl", format!("{:.2}", m.pnl), format!("{:.2}", bm.pnl));
    row(
        "return",
        format!("{:.2}%", m.total_return_pct),
        format!("{:.2}%", bm.total_return_pct),
    );
    row("sharpe", fmt_opt(m.sharpe, ""), fmt_opt(bm.sharpe, ""));
    row(
        "max drawdown",
        format!("{:.2}%", m.max_drawdown_pct),
        format!("{:.2}%", bm.max_drawdown_pct),
    );
    row(
        "exposure",
        format!("{:.1}%", m.exposure_pct),
        format!("{:.1}%", bm.exposure_pct),
    );
    row("trades", m.trades.to_string(), bm.trades.to_string());
    row(
        "win rate",
        fmt_opt(m.win_rate_pct, "%"),
        fmt_opt(bm.win_rate_pct, "%"),
    );
    row(
        "avg win",
        format!("{:.2}", m.avg_win),
        format!("{:.2}", bm.avg_win),
    );
    row(
        "avg loss",
        format!("{:.2}", m.avg_loss),
        format!("{:.2}", bm.avg_loss),
    );
    row(
        "profit factor",
        fmt_opt(m.profit_factor, ""),
        fmt_opt(bm.profit_factor, ""),
    );
    row(
        "fees paid",
        format!("{:.2}", m.fees_paid),
        format!("{:.2}", bm.fees_paid),
    );

    Ok(())
}
