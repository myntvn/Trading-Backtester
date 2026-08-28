use anyhow::{Result, ensure};

use crate::{data::Bar, indicators::ema, strategy::Signal::Long};

/// The position a strategy wants to hold as of a given bar.
///
/// This is desired *state*, not an order. The engine derives orders by
/// diffing consecutive signals.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Signal {
    Flat,
    Long,
}

pub trait Strategy {
    /// Human-readable name, including parameters.
    fn name(&self) -> String;

    /// One signal per bar. `signals[i]` may only depend on bars `0..=i`.
    fn signals(&self, bars: &[Bar]) -> Vec<Signal>;
}

// Long while the fast EMA is above the slow EMA, flat otherwise.
pub struct EmaCross {
    fast: usize,
    slow: usize,
}

impl EmaCross {
    pub fn new(fast: usize, slow: usize) -> Result<Self> {
        ensure!(fast > 0, "fast period must be greater than zero");
        ensure!(
            fast < slow,
            "fast period ({fast}) must be less than slow period ({slow})"
        );

        Ok(Self { fast, slow })
    }
}

impl Strategy for EmaCross {
    fn name(&self) -> String {
        format!("EMA({}, {}) cross", self.fast, self.slow)
    }

    fn signals(&self, bars: &[Bar]) -> Vec<Signal> {
        let closes: Vec<f64> = bars.iter().map(|b| b.close).collect();

        let fast = ema(&closes, self.fast);
        let slow = ema(&closes, self.slow);

        fast.iter()
            .copied()
            .zip(slow.iter().copied())
            .map(|(f, s)| match (f, s) {
                (Some(f), Some(s)) if f > s => Signal::Long,
                _ => Signal::Flat,
            })
            .collect()
    }
}

/// Always long. The benchmark, run through the same engine so that fees and
/// fill timing match the strategy exactly.
pub struct BuyHold;

impl Strategy for BuyHold {
    fn name(&self) -> String {
        "buy and hold".to_string()
    }

    fn signals(&self, bars: &[Bar]) -> Vec<Signal> {
        vec![Long; bars.len()]
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Build bars from a close-price path. Every OHLC field is the close,
    /// which is all this strategy reads.
    fn bars_from(closes: &[f64]) -> Vec<Bar> {
        closes
            .iter()
            .enumerate()
            .map(|(i, &c)| Bar {
                ts: i as i64,
                open: c,
                high: c,
                low: c,
                close: c,
                volume: 0.0,
            })
            .collect()
    }

    fn ramp(start: f64, step: f64, n: usize) -> Vec<f64> {
        (0..n).map(|i| start + step * i as f64).collect()
    }

    #[test]
    fn rejects_bad_parameters() {
        assert!(EmaCross::new(0, 20).is_err());
        assert!(EmaCross::new(20, 20).is_err());
        assert!(EmaCross::new(50, 20).is_err());
        assert!(EmaCross::new(20, 50).is_ok());
    }

    #[test]
    fn warmup_is_flat_then_turns_long() {
        let bars = bars_from(&ramp(100.0, 1.0, 20));
        let sigs = EmaCross::new(3, 5).unwrap().signals(&bars);

        // The slow EMA seeds at index 4, so nothing before it can be Long.
        assert!(sigs[..4].iter().all(|&s| s == Signal::Flat));
        assert_eq!(sigs[4], Signal::Long);
    }

    #[test]
    fn uptrend_goes_long() {
        let bars = bars_from(&ramp(100.0, 2.0, 60));
        let sigs = EmaCross::new(20, 50).unwrap().signals(&bars);

        assert_eq!(*sigs.last().unwrap(), Signal::Long);
    }

    #[test]
    fn downtrend_stays_flat() {
        let bars = bars_from(&ramp(100.0, -1.0, 60));
        let sigs = EmaCross::new(20, 50).unwrap().signals(&bars);

        assert!(sigs.iter().all(|&s| s == Signal::Flat));
    }

    #[test]
    fn reversal_exits_the_position() {
        let mut closes = ramp(100.0, 2.0, 60);
        closes.extend(ramp(220.0, -3.0, 60));
        let bars = bars_from(&closes);

        let sigs = EmaCross::new(10, 30).unwrap().signals(&bars);

        assert!(sigs.contains(&Signal::Long));
        assert_eq!(*sigs.last().unwrap(), Signal::Flat);
    }

    #[test]
    fn signal_length_matches_bar_count() {
        let bars = bars_from(&ramp(100.0, 1.0, 37));
        assert_eq!(EmaCross::new(3, 5).unwrap().signals(&bars).len(), 37);
    }

    #[test]
    fn buy_hold_is_always_long() {
        let bars = bars_from(&ramp(100.0, -1.0, 10));
        let sigs = BuyHold.signals(&bars);

        assert_eq!(sigs.len(), bars.len());
        assert!(sigs.iter().all(|&s| s == Long));
    }
}
