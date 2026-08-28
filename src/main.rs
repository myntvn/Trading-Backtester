use std::path::PathBuf;

use anyhow::{Context, Result};

use crate::{
    data::fmt_date,
    engine::EngineConfig,
};

mod data;
mod engine;
mod indicators;
mod strategy;
mod metrics;

const FAST: usize = 20;
const SLOW: usize = 50;

fn main() -> Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .context("usage: backtester <path-to-csv>")?
        .into();

    let bars = data::load_csv(&path)?;
    let strat = strategy::EmaCross::new(FAST, SLOW)?;
    let cfg = EngineConfig::default();

    let bt = engine::run(&bars, &strat, &cfg)?;

    println!("{}", bt.strategy);
    println!("{} bars, {:.1} bps fees\n", bars.len(), cfg.fee_bps);

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

    let wins = bt.trades.iter().filter(|t| t.is_win()).count();
    let fees: f64 = bt.trades.iter().map(|t| t.fees).sum();

    println!();
    println!("trades     {:>12}", bt.trades.len());
    println!("wins       {:>12}", wins);
    println!("fees paid  {:>12.2}", fees);
    println!("initial    {:>12.2}", bt.initial_cash);
    println!("final      {:>12.2}", bt.final_equity());
    println!("return     {:>11.2}%", bt.total_return_pct());

    Ok(())
}
