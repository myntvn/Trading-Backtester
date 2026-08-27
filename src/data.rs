use anyhow::{Context, Result, ensure};
use chrono::{DateTime, TimeZone, Utc};
use std::path::Path;

#[derive(Debug, Clone, Copy)]
pub struct Bar {
    pub ts: i64, // open_time, milliseconds since epoch
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

impl Bar {
    #[allow(dead_code)]
    pub fn datetime(&self) -> DateTime<Utc> {
        Utc.timestamp_millis_opt(self.ts)
            .single()
            .expect("timestamp out of range")
    }
}

/// Binance switched some archives from milliseconds to microseconds.
/// Anything past ~5138 AD in millis is really micros.
fn normalize_ts(raw: i64) -> i64 {
    if raw > 100_000_000_000_000 {
        raw / 1000
    } else {
        raw
    }
}

fn parse_row(rec: &csv::StringRecord) -> Result<Bar> {
    let field =
        |i: usize| -> Result<&str> { Ok(rec.get(i).context("row has too few columns")?.trim()) };

    let ts: i64 = field(0)?.parse().context("open_time")?;

    Ok(Bar {
        ts: normalize_ts(ts),
        open: field(1)?.parse().context("open")?,
        high: field(2)?.parse().context("high")?,
        low: field(3)?.parse().context("low")?,
        close: field(4)?.parse().context("close")?,
        volume: field(5)?.parse().context("volume")?,
    })
}

/// Load Binance-format kline CSV rows into bars, sorted by time.
pub fn load_csv(path: &Path) -> Result<Vec<Bar>> {
    let mut rdr = csv::ReaderBuilder::new()
        .has_headers(false)
        .flexible(true)
        .from_path(path)
        .with_context(|| format!("opening {}", path.display()))?;

    let mut bars = Vec::new();
    for (i, result) in rdr.records().enumerate() {
        let rec = result.with_context(|| format!("reading row {}", i + 1))?;

        match parse_row(&rec) {
            Ok(bar) => bars.push(bar),
            Err(_) if i == 0 => continue,

            Err(e) => return Err(e).with_context(|| format!("row {}", i + 1)),
        }
    }

    ensure!(!bars.is_empty(), "no bars parsed from {}", path.display());

    bars.sort_by_key(|b| b.ts);

    Ok(bars)
}

/// Format a millisecond timestamp as `YYYY-MM-DD`.
pub fn fmt_date(ts: i64) -> String {
    Utc.timestamp_millis_opt(ts)
        .single()
        .map(|d| d.format("%Y-%m-%d").to_string())
        .unwrap_or_else(|| ts.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_microsecond_timestamps() {
        assert_eq!(normalize_ts(1_704_067_200_000), 1_704_067_200_000);
        assert_eq!(normalize_ts(1_704_067_200_000_000), 1_704_067_200_000);
    }
}
