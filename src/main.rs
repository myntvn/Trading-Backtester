use std::path::PathBuf;

use anyhow::{Context, Result};

mod data;

fn main() -> Result<()> {
    let path: PathBuf = std::env::args()
    .nth(1)
    .context("usage: backtester <path-to-csv>")?
    .into();

    let bars = data::load_csv(&path)?;

    // Safe: load_csv guarantees at least one bar.
    let first = bars.first().unwrap();
    let last = bars.last().unwrap();

    println!("loaded {} bars", bars.len());
    println!(
        "first {} close {:.2}",
        first.datetime().format("%Y-%m-%d"),
        first.close
    );
   println!(
        "last {} close {:.2}",
        last.datetime().format("%Y-%m-%d"),
        last.close
    ); 

    Ok(())
}
