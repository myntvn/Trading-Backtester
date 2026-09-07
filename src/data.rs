use anyhow::{Context, Result, bail, ensure};
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

    /// Whether this is a well-formed candle.
    ///
    /// Catches malformed exchange data: an inverted range, a price outside
    /// its own high/low, or a non-positive price.
    fn is_coherent(&self) -> bool {
        self.low > 0.0
            && self.high >= self.low
            && self.high >= self.open.max(self.close)
            && self.low <= self.open.min(self.close)
            && self.volume >= 0.0
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
            Ok(bar) if bar.is_coherent() => bars.push(bar),

            Ok(bar) => bail!("row {} is not a coherent candle: {bar:?}", i + 1),

            // Tolerate a header on the very first row; any other bad row is a real bug.
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

    fn candle(open: f64, high: f64, low: f64, close: f64, volume: f64) -> Bar {
        Bar {
            ts: 0,
            open,
            high,
            low,
            close,
            volume,
        }
    }

    #[test]
    fn normalizes_microsecond_timestamps() {
        assert_eq!(normalize_ts(1_704_067_200_000), 1_704_067_200_000);
        assert_eq!(normalize_ts(1_704_067_200_000_000), 1_704_067_200_000);
    }

    #[test]
    fn accepts_a_well_formed_candle() {
        assert!(candle(100.0, 110.0, 95.0, 105.0, 12.0).is_coherent());
    }

    #[test]
    fn accepts_a_flat_candle() {
        // All four prices equal is legal — a bar with no movement.
        assert!(candle(100.0, 100.0, 100.0, 100.0, 0.0).is_coherent());
    }

    #[test]
    fn rejects_an_inverted_range() {
        assert!(!candle(100.0, 95.0, 110.0, 105.0, 12.0).is_coherent());
    }

    #[test]
    fn rejects_a_close_above_the_high() {
        assert!(!candle(100.0, 110.0, 95.0, 115.0, 12.0).is_coherent());
    }

    #[test]
    fn rejects_an_open_below_the_low() {
        assert!(!candle(90.0, 110.0, 95.0, 105.0, 12.0).is_coherent());
    }

    #[test]
    fn rejects_non_positive_prices() {
        assert!(!candle(0.0, 110.0, 0.0, 105.0, 12.0).is_coherent());
        assert!(!candle(100.0, 110.0, -5.0, 105.0, 12.0).is_coherent());
    }

    #[test]
    fn rejects_negative_volume() {
        assert!(!candle(100.0, 110.0, 95.0, 105.0, -1.0).is_coherent());
    }
}
