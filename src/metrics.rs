use crate::engine::Backtest;

#[derive(Debug, Clone)]
pub struct Metrics {
    pub pnl: f64,
    pub total_return_pct: f64,
    pub final_equity: f64,
    /// Annuallised, zero risk-free rate. `None` when undefined.
    pub sharpe: Option<f64>,
    pub max_drawdown_pct: f64,
    pub trades: usize,
    pub wins: usize,
    pub losses: usize,
    /// `None` when there were no trades — not the same as 0%.
    pub win_rate_pct: Option<f64>,
    pub avg_win: f64,
    pub avg_loss: f64,
    ///  Gross wins / gross losses. `None` when there were no losses.
    pub profit_factor: Option<f64>,
    /// Share of bars spent holding a position.
    pub exposure_pct: f64,
    pub fees_paid: f64,
}

/// Per-bar simple returns of an equity curve. Length is `equity.len() - 1`.
fn returns(equity: &[f64]) -> Vec<f64> {
    equity
        .windows(2)
        .map(|w| if w[0] == 0.0 { 0.0 } else { w[1] / w[0] - 1.0 })
        .collect()
}

/// Annualised Sharpe ratio, assuming a zero risk-free rate.
///
/// `None` when there is too little data, or when the returns have no
/// variance at all — a strategy that never trades has no Sharpe, which is
/// different from having a Sharpe of zero.
fn sharpe(returns: &[f64], periods_per_year: f64) -> Option<f64> {
    if returns.len() < 2 {
        return None;
    }

    let n = returns.len() as f64;
    let mean = returns.iter().sum::<f64>() / n;

    // Sample variance: divide by n-1 (Bessel's correction), because these
    // returns are a sample of the process, not the whole population.
    let variance = returns.iter().map(|r| (r - mean).powi(2)).sum::<f64>() / (n - 1.0);
    let sd = variance.sqrt();

    if sd == 0.0 || sd.is_infinite() {
        return None;
    }

    Some(mean / sd * periods_per_year.sqrt())
}

/// Worst peak-to-trough decline, as a positive percentage.
fn max_drawdown_pct(equity: &[f64]) -> f64 {
    let mut peak = f64::MIN;
    let mut worst = 0.0_f64;

    for &e in equity {
        peak = peak.max(e);
        if peak > 0.0 {
            worst = worst.max((peak - e) / peak);
        }
    }

    worst * 100.0
}

pub fn compute(bt: &Backtest, periods_per_year: f64) -> Metrics {
    let rets = returns(&bt.equity);

    let wins: Vec<f64> = bt
        .trades
        .iter()
        .filter(|t| t.is_win())
        .map(|t| t.pnl)
        .collect();

    let losses: Vec<f64> = bt
        .trades
        .iter()
        .filter(|t| !t.is_win())
        .map(|t| t.pnl)
        .collect();

    let gross_win: f64 = wins.iter().sum();
    let gross_loss: f64 = losses.iter().sum::<f64>().abs();

    let mean_or_zero = |v: &[f64]| {
        if v.is_empty() {
            0.0
        } else {
            v.iter().sum::<f64>() / v.len() as f64
        }
    };

    let final_equity = bt.final_equity();

    Metrics {
        pnl: final_equity - bt.initial_cash,
        total_return_pct: bt.total_return_pct(),
        final_equity,
        sharpe: sharpe(&rets, periods_per_year),
        max_drawdown_pct: max_drawdown_pct(&bt.equity),
        trades: bt.trades.len(),
        wins: wins.len(),
        losses: losses.len(),
        win_rate_pct: if bt.trades.is_empty() {
            None
        } else {
            Some(wins.len() as f64 / bt.trades.len() as f64 * 100.0)
        },
        avg_win: mean_or_zero(&wins),
        avg_loss: mean_or_zero(&losses),
        profit_factor: if gross_loss == 0.0 {
            None
        } else {
            Some(gross_win / gross_loss)
        },
        exposure_pct: if bt.equity.is_empty() {
            0.0
        } else {
            bt.bars_in_market as f64 / bt.equity.len() as f64 * 100.0
        },
        fees_paid: bt.trades.iter().map(|t| t.fees).sum(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::engine::Trade;

    /// A `Backtest` assembled by hand, so each metric is tested in isolation.
    fn bt(equity: Vec<f64>, pnls: &[f64]) -> Backtest {
        let initial_cash = equity.first().copied().unwrap_or(0.0);
        let trades = pnls
            .iter()
            .map(|&pnl| Trade {
                entry_ts: 0,
                exit_ts: 1,
                entry_price: 100.0,
                exit_price: 100.0 + pnl,
                qty: 1.0,
                fees: 0.0,
                pnl,
                forced_exit: false,
            })
            .collect();

        Backtest {
            strategy: "test".to_string(),
            equity,
            trades,
            initial_cash,
            bars_in_market: 0,
        }
    }

    fn approx(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn returns_are_simple_period_returns() {
        let r = returns(&[100.0, 110.0, 143.0]);

        assert_eq!(r.len(), 2);
        assert!(approx(r[0], 0.1));
        assert!(approx(r[1], 0.3));
    }

    #[test]
    fn sharpe_matches_hand_computed_value() {
        // returns [0.1, 0.3] -> mean 0.2, sample sd sqrt(0.02)
        // ratio = 0.2 / sqrt(0.02) = sqrt(2)
        let m = compute(&bt(vec![100.0, 110.0, 143.0], &[]), 1.0);
        assert!(approx(m.sharpe.unwrap(), 2.0_f64.sqrt()));
    }

    #[test]
    fn sharpe_applies_the_annualisation_factor() {
        let m = compute(&bt(vec![100.0, 110.0, 143.0], &[]), 365.0);
        assert!(approx(m.sharpe.unwrap(), 2.0_f64.sqrt() * 365.0_f64.sqrt()));
    }

    #[test]
    fn symmetric_returns_give_zero_sharpe() {
        // [0.1, -0.1] -> mean 0 -> Sharpe is exactly 0, which is NOT None.
        let m = compute(&bt(vec![100.0, 110.0, 99.0], &[]), 365.0);
        assert!(approx(m.sharpe.unwrap(), 0.0));
    }

    #[test]
    fn zero_variance_has_no_sharpe() {
        let m = compute(&bt(vec![100.0, 10.0], &[]), 365.0);
        assert_eq!(m.sharpe, None);
    }

    #[test]
    fn too_few_returns_has_no_sharpe() {
        assert_eq!(sharpe(&[], 365.0), None);
        assert_eq!(sharpe(&[0.01], 365.0), None);
    }

    #[test]
    fn max_drawdown_is_peak_to_trough() {
        // peak 120, trough 60 -> 50%
        assert!(approx(max_drawdown_pct(&[100.0, 120.0, 60.0, 80.0]), 50.0));
    }

    #[test]
    fn rising_equity_has_no_drawdown() {
        assert!(approx(max_drawdown_pct(&[100.0, 110.0, 120.0]), 0.0));
    }

    #[test]
    fn win_rate_counts_profitable_trades() {
        let m = compute(&bt(vec![100.0, 110.0], &[10.0, -5.0, 20.0]), 365.0);

        assert_eq!((m.trades, m.wins, m.losses), (3, 2, 1));
        assert!(approx(m.win_rate_pct.unwrap(), 200.0 / 3.0));
        assert!(approx(m.avg_win, 15.0));
        assert!(approx(m.avg_loss, -5.0));
        assert!(approx(m.profit_factor.unwrap(), 6.0)) // 30 / 5
    }

    #[test]
    fn no_trades_leaves_trade_stats_undefined() {
        let m = compute(&bt(vec![100.0, 100.0], &[]), 365.0);

        assert_eq!(m.trades, 0);
        assert_eq!(m.win_rate_pct, None);
        assert_eq!(m.profit_factor, None);
    }

    #[test]
    fn all_winners_have_no_profit_factor() {
        let m = compute(&bt(vec![100.0, 110.0], &[10.0, 20.0]), 365.0);

        assert_eq!(m.profit_factor, None);
        assert!(approx(m.win_rate_pct.unwrap(), 100.0));
    }

    #[test]
    fn pnl_and_return_come_from_the_equity_curve() {
        let m = compute(&bt(vec![100.0, 125.0], &[]), 365.0);

        assert!(approx(m.pnl, 25.0));
        assert!(approx(m.total_return_pct, 25.0));
        assert!(approx(m.final_equity, 125.0));
    }
}
