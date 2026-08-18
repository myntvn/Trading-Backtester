use std::path::PathBuf;

use anyhow::{Context, Result};

mod data;
mod indicators;

const FAST: usize = 20;
const SLOW: usize = 50;

fn fmt_opt(v: Option<f64>) -> String {
    match v {
        Some(x) => format!("{x:>10.2}"),
        None => format!("{:>10}", "-"),
    }
}

fn main() -> Result<()> {
    let path: PathBuf = std::env::args()
        .nth(1)
        .context("usage: backtester <path-to-csv>")?
        .into();

    let bars = data::load_csv(&path)?;
    let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();

    let fast = indicators::ema(&closes, FAST);
    let slow = indicators::ema(&closes, SLOW);

    println!("loaded {} bars", bars.len());
    println!(
        "{:<12}{:>10}{:>10}{:>10}",
        "date",
        "close",
        format!("ema{FAST}"),
        format!("ema{SLOW}")
    );

    let start = bars.len().saturating_sub(10);
    for i in start..bars.len() {
        println!(
            "{:<12}{:>10.2}{}{}",
            bars[i].datetime().format("%Y-%m-%d").to_string(),
            bars[i].close,
            fmt_opt(fast[i]),
            fmt_opt(slow[i]),
        );
    }

    Ok(())
}
