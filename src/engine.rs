use anyhow::{Ok, Result, ensure};

use crate::{
    data::Bar,
    strategy::{Signal, Strategy},
};

pub struct EngineConfig {
    pub initial_cash: f64,

    /// Round-trip cost per side, in basis points (1 bp = 0.01%).
    pub free_bps: f64,
}

impl Default for EngineConfig {
    fn default() -> Self {
        Self {
            initial_cash: 10_000.0,
            free_bps: 10.0,
        }
    }
}

/// A completed round trip.
#[derive(Debug, Clone, Copy)]
pub struct Trade {
    pub entry_ts: i64,
    pub exit_ts: i64,
    pub entry_price: f64,
    pub exit_price: f64,
    pub qty: f64,
    /// Both side combined.
    pub fees: f64,
    /// Net of fees.
    pub pnl: f64,
    /// True if the position was still open on the final bar and was marked out.
    pub force_exit: bool,
}

impl Trade {
    pub fn return_pct(&self) -> f64 {
        let notional = self.entry_price * self.qty;
        if notional == 0.0 {
            0.0
        } else {
            self.pnl / notional * 100.0
        }
    }

    pub fn is_win(&self) -> bool {
        self.pnl > 0.0
    }
}

pub struct Backtest {
    pub strategy: String,
    /// One entry per bar, mark-to-market on the close.
    pub equity: Vec<f64>,
    pub trades: Vec<Trade>,
    pub initial_cash: f64,
}

impl Backtest {
    pub fn final_equity(&self) -> f64 {
        self.equity.last().copied().unwrap_or(self.initial_cash)
    }

    pub fn total_return_pct(&self) -> f64 {
        (self.final_equity() / self.initial_cash - 1.0) * 100.0
    }
}

struct OpenPos {
    entry_ts: i64,
    entry_price: f64,
    qty: f64,
    /// Cash actually spent, fee included.
    entry_cost: f64,
    entry_fee: f64,
}

struct Portfolio {
    cash: f64,
    fee_rate: f64,
    open: Option<OpenPos>,
}

impl Portfolio {
    fn new(cash: f64, fee_bps: f64) -> Self {
        Self {
            cash,
            fee_rate: fee_bps / 10_000.0,
            open: None,
        }
    }

    fn qty(&self) -> f64 {
        self.open.as_ref().map_or(0.0, |p| p.qty)
    }

    fn equity(&self, price: f64) -> f64 {
        self.cash + self.qty() * price
    }

    /// Spend all available cash at `price`.
    ///
    /// Solving `cash = qty*price*(1 + fee_rate)` for `qty` keeps the spend
    /// exactly equal to the cash on hand, fee included.
    fn buy(&mut self, ts: i64, price: f64) {
        debug_assert!(self.open.is_none(), "buy() called while already long");

        let qty = self.cash / (price * (1.0 + self.fee_rate));
        let notional = qty * price;
        let fee = notional * self.fee_rate;
        let cost = notional + fee;

        self.cash -= cost;
        self.open = Some(OpenPos {
            entry_ts: ts,
            entry_price: price,
            qty,
            entry_cost: cost,
            entry_fee: fee,
        })
    }

    /// Close the open position at `price`. Returns `None` if already flat.
    fn sell(&mut self, ts: i64, price: f64, forced: bool) -> Option<Trade> {
        let pos = self.open.take()?;

        let notional = pos.qty * price;
        let fee = notional * self.fee_rate;
        let proceeds = notional - fee;

        self.cash += proceeds;

        Some(Trade {
            entry_ts: pos.entry_ts,
            exit_ts: ts,
            entry_price: pos.entry_price,
            exit_price: price,
            qty: pos.qty,
            fees: pos.entry_fee + fee,
            pnl: proceeds - pos.entry_cost,
            force_exit: forced,
        })
    }
}

pub fn run(bars: &[Bar], strat: &dyn Strategy, cfg: &EngineConfig) -> Result<Backtest> {
    ensure!(bars.len() >= 2, "need at least 2 bars to run a backtest");
    ensure!(cfg.initial_cash > 0.0, "initial cash must be positive");
    ensure!(cfg.free_bps >= 0.0, "fee_bps cannot be negative");

    let signals = strat.signals(bars);
    ensure!(
        signals.len() == bars.len(),
        "strategy returned {} signals for {} bars",
        signals.len(),
        bars.len()
    );

    let mut pf = Portfolio::new(cfg.initial_cash, cfg.free_bps);
    let mut trades = Vec::new();
    let mut equity = Vec::with_capacity(bars.len());

    // Bar 0: nothing can have been decided yet.
    equity.push(pf.equity(bars[0].close));

    for i in 1..bars.len() {
        let bar = &bars[i];

        // The signal from the PREVIOUS close is the newest information
        // available at this bar's open. Using signals[i] here would be
        // lookahead bias.
        let desired = signals[i - 1];

        match (pf.open.is_some(), desired) {
            (false, Signal::Long) => pf.buy(bar.ts, bar.open),
            (true, Signal::Flat) => trades.extend(pf.sell(bar.ts, bar.open, false)),
            _ => {}
        }

        equity.push(pf.equity(bar.close));
    }

    // Mark out a position still open on the final bar. Every open is behind
    // us, so the final close is the only honest price left.
    if pf.open.is_some() {
        let last = bars.last().expect("bars is non-empty");
        trades.extend(pf.sell(last.ts, last.close, true));
        *equity.last_mut().expect("equity is non-empty") = pf.equity(last.close);
    }

    Ok(Backtest {
        strategy: strat.name(),
        equity,
        trades,
        initial_cash: cfg.initial_cash,
    })
}
