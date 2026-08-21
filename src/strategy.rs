use anyhow::{Result, ensure};

use crate::{data::Bar, indicators::ema};

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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_bad_parameters() {
        assert!(EmaCross::new(0, 20).is_err());
        assert!(EmaCross::new(20, 20).is_err());
        assert!(EmaCross::new(50, 20).is_err());
        assert!(EmaCross::new(20, 50).is_ok());
    }
}
