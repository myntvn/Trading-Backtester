# backtester

A trading backtester in Rust. Reads OHLCV candles, runs an EMA-crossover
strategy against them, and reports PnL, Sharpe ratio, win rate, drawdown and
full trade history — always alongside a buy & hold benchmark computed through
the same execution model.

Built as a learning project. The emphasis throughout is on *not lying to
yourself*: no lookahead bias, fees on every fill, a fair benchmark, and a
train/test split for parameter selection.

```
1827 bars   2020-01-01 -> 2024-12-31   10.0 bps fees

trade history
entry        exit                 in        out         pnl     ret%
2020-02-20   2020-03-11      9594.65    7894.57    -1788.34  -17.90%
2020-04-28   2020-09-11      7773.51   10336.86     2686.01   32.74%
2020-10-11   2021-05-15     11293.22   49844.16    37104.57  340.82%
2021-08-01   2021-09-28     41461.84   42147.35      696.15    1.45%
...

                 EMA(20, 50) cross      buy and hold
----------------------------------------------------
final equity              94800.64         129693.12
return                     848.01%          1196.93%
sharpe                        1.19              1.12
max drawdown                51.58%            76.63%
exposure                     58.2%             99.9%
```

## Quick start

```bash
# 1. Get data (Binance public archive, no auth required)
mkdir -p data && cd data
for y in 2020 2021 2022 2023 2024; do
  for m in 01 02 03 04 05 06 07 08 09 10 11 12; do
    curl -sO "https://data.binance.vision/data/spot/monthly/klines/BTCUSDT/1d/BTCUSDT-1d-$y-$m.zip"
  done
done
unzip -oq '*.zip'
cat BTCUSDT-1d-20*.csv | sort -n -t, -k1 > btc-2020-2024.csv
cd ..

# 2. Single backtest
cargo run --release -- data/btc-2020-2024.csv --fast 20 --slow 50

# 3. Parameter sweep with train/test split
cargo run --release -- data/btc-2020-2024.csv --sweep
```

Use `--release` for sweeps. Debug builds are ~20x slower due to bounds
checking, which is noticeable once the grid grows.

## Input format

Binance kline CSV: 12 comma-separated columns, no header row.

```
open_time(ms), open, high, low, close, volume, close_time,
quote_volume, trades, taker_buy_base, taker_buy_quote, ignore
```

Only the first six are read. The loader:

- tolerates a header row if one is present (first row only)
- normalises microsecond timestamps to milliseconds (Binance switched formats
  in some archives)
- sorts by timestamp
- rejects incoherent candles (inverted range, price outside its own high/low,
  non-positive price, negative volume)

Any parse failure after row 0 is a hard error, not a skipped row.

## CLI

```
backtester [OPTIONS] <CSV>
```

| Option | Default | Meaning |
|---|---|---|
| `<CSV>` | required | Path to a Binance-format kline CSV |
| `--fast <N>` | `20` | Fast EMA period |
| `--slow <N>` | `50` | Slow EMA period |
| `--cash <F>` | `10000` | Starting capital |
| `--fees <F>` | `10` | Per-side fee in basis points (1 bp = 0.01%) |
| `--periods-per-year <F>` | `365` | Sharpe annualisation (365 daily crypto, 252 equities) |
| `--no-benchmark` | off | Drop the buy & hold column |
| `--sweep` | off | Run a parameter sweep instead of one backtest |
| `--fast-range <LIST>` | `5,8,10,12,15,20,25,30` | Fast periods to try |
| `--slow-range <LIST>` | `20,30,40,50,80,100,150,200` | Slow periods to try |
| `--split <F>` | `0.7` | Fraction of bars used for training |
| `--top <N>` | `15` | Sweep results to display |

Examples:

```bash
# Faster crossover, no benchmark column
cargo run --release -- data/btc-2020-2024.csv --fast 8 --slow 100 --no-benchmark

# Higher fees, to test whether a result survives realistic execution costs
cargo run --release -- data/btc-2020-2024.csv --sweep --fees 25

# Custom grid, 80/20 split, top 5 only
cargo run --release -- data/btc-2020-2024.csv --sweep \
  --fast-range 5,10,15 --slow-range 50,100,200 --split 0.8 --top 5
```

## Architecture

```
CSV ──▶ Vec<Bar> ──▶ EMA ──▶ Vec<Signal> ──▶ Engine ──▶ Backtest ──▶ Metrics ──▶ report
        data.rs      indicators   strategy      engine    (trades +    metrics     report
                                                           equity)
                                                              │
                                       sweep.rs ──────────────┘
                                    (train/test, rayon)
```

Each stage is a pure function of the stage before it. Nothing shares mutable
state, which is what makes the sweep parallelisable with a one-word change
(`iter()` -> `par_iter()`) and no locks.

### Modules

| File | Lines | Responsibility |
|---|---|---|
| `data.rs` | 158 | `Bar` type, CSV loading, timestamp handling, candle validation |
| `indicators.rs` | 74 | `ema()` — index-aligned, `None` during warm-up |
| `strategy.rs` | 162 | `Signal`, `Strategy` trait, `EmaCross`, `BuyHold` |
| `engine.rs` | 368 | Execution loop, `Portfolio`, fills, fees, `Trade`, `Backtest` |
| `metrics.rs` | 262 | Sharpe, drawdown, win rate, profit factor, exposure |
| `sweep.rs` | 269 | Cartesian parameter grid, train/test split, parallel execution, ranking |
| `report.rs` | 146 | Terminal tables |
| `main.rs` | 182 | `clap` argument parsing, wiring |

## Design decisions

**No lookahead bias.** `signals[i]` is computed from bar `i`'s close, and the
earliest possible fill is bar `i+1`'s open. The engine loop is indexed
accordingly:

```rust
let desired = signals[i - 1];   // decided at the previous close
// ... fill at bars[i].open
```

Two consequences that are correct and deliberate: the final bar's signal is
never acted on (there is no next bar to fill at), and bar 0 never trades.

**Signals are state, not events.** `Signal` is `Flat | Long` — the position
you *want to hold*, not an order. The engine derives orders by diffing
consecutive signals. This makes each signal independently checkable, and
means there is no `Hold` variant anywhere in the codebase.

**Fees from the start.** `--fees` defaults to 10 bps per side (roughly Binance
spot taker). All-in position sizing solves `cash = qty * price * (1 + fee)`
for `qty`, so the spend exactly equals available cash rather than overshooting
it. Trade PnL is `exit_proceeds - entry_cost` with fees baked into both, which
makes it impossible to double-count or miss them.

**The benchmark runs through the same engine.** `BuyHold` is a `Strategy` that
returns `Long` for every bar. Fill timing, fees and the final mark-out are
therefore identical to the strategy's by construction. Computing buy & hold as
`last_close / first_close - 1` would silently pay no fees and fill at a price
the engine would never have given.

**Open positions are marked out at the final close.** A position still open on
the last bar is closed at that bar's close and flagged `forced_exit`. The
equity curve already carries the mark-to-market value at every bar, so it is
trustworthy mid-trade.

**`None` means undefined, not zero.** `sharpe`, `win_rate_pct`,
`profit_factor`, `avg_win` and `avg_loss` are `Option<f64>`. A strategy that
never traded has *no* Sharpe — distinct from a Sharpe of 0.0, which means "no
edge". These print as `n/a`.

**Validation lives at the boundary of the thing it protects.** clap handles
types, `EmaCross::new` enforces `0 < fast < slow`, `engine::run` enforces its
own preconditions. None of these are hoisted into argument parsing, so they
still protect tests and the sweep.

## Metric definitions

| Metric | Definition |
|---|---|
| return | `final_equity / initial_cash - 1` |
| sharpe | `mean(r) / stddev(r) * sqrt(periods_per_year)`, zero risk-free rate, sample stddev (Bessel), on per-bar simple returns of the equity curve |
| max drawdown | worst peak-to-trough decline of the equity curve, positive % |
| exposure | share of bars holding a position |
| win rate | trades with `pnl > 0`, over all trades |
| profit factor | gross wins / gross losses; `None` when there are no losses |
| trades | completed round trips, not fills |

Sharpe caveats worth remembering: flat periods contribute exactly-zero
returns, which shrinks stddev without shrinking the mean proportionally, so a
low-exposure strategy gets a flattering Sharpe. Always read it next to
`exposure`. And a Sharpe estimated from one year of daily bars has a standard
error near ±1.0 — it is useful for comparing configurations on the same data,
not as evidence about the future.

## Findings

Measured on BTCUSDT daily bars, 10 bps per side.

**Train 2020-01 -> 2023-07** (bull, crash, bear), `--fast 8 --slow 100`:

| | strategy | buy & hold |
|---|---|---|
| return | 587.19% | 323.91% |
| sharpe | 1.40 | 0.95 |
| max drawdown | 35.55% | 76.63% |

**Test 2023-07 -> 2024-12** (recovery/bull), same parameters:

| | strategy | buy & hold |
|---|---|---|
| return | 169.64% | 205.02% |
| sharpe | 1.78 | 1.77 |
| max drawdown | 31.56% | 26.15% |

**2022 in isolation** (bear):

| | strategy | buy & hold |
|---|---|---|
| return | -5.45% | -65.41% |
| max drawdown | 6.33% | 66.96% |
| exposure | 1.1% | 99.7% |

The pattern is consistent across every window tested:

- In **bull-only** periods the strategy loses to buy & hold or ties it.
- In periods containing a **bear market** it beats buy & hold decisively, with
  less than half the drawdown.

This is trend-following behaving as designed — it pays a premium during
sustained rallies in exchange for not being present when the market falls 65%.
Whether that trade is worth making is a risk-tolerance question, not a
question about the code.

**The parameter plateau moved when more data was added.** On 2024 alone the
best region was fast 5–15 / slow 20–40. Across 2020–2024 it is fast 5–15 /
slow 80–100. A contiguous plateau rules out a lucky single grid cell; it does
not rule out a lucky single period. Only more history does that.

**Caveat on trade counts.** The test window produces 3 trades and 2022
produces 1. Sharpe is computed from ~550 daily returns, but those returns are
a handful of dependent holding blocks, so the effective sample size for
judging the strategy is far smaller than the return count suggests.

## Testing

```bash
cargo test                    # 55 tests
cargo clippy --all-targets    # clean
cargo fmt --check
```

Coverage by module: `data` 8, `engine` 8, `indicators` 5, `metrics` 12,
`strategy` 7, `sweep` 11, CLI 4.

Conventions used throughout:

- **Hand-computed expected values.** The EMA is checked against
  `[1,2,3,4,5]` with period 3, where `k = 0.5` makes every result exactly
  representable in binary floating point. Sharpe is checked against an equity
  curve of `[100, 110, 143]`, whose returns `[0.1, 0.3]` give exactly
  `sqrt(2)` at `periods_per_year = 1.0`.
- **A stub `Strategy`** (`Fixed(Vec<Signal>)`) drives the engine with an exact
  signal sequence, so execution can be tested without reverse-engineering
  price data that produces the desired crossovers.
- **An explicit lookahead test.** A `Long` on the final bar must produce zero
  trades.
- **Float comparison via an `approx` helper** (1e-9), except where values are
  exactly representable, and `to_bits()` for the sweep determinism test where
  bitwise identity is the actual claim.
- **Structure, not numbers, for the sweep** — combination counts, split
  boundaries, ranking order and determinism. The arithmetic is already covered
  by the engine and metrics tests.

## Known limitations

- **Long-only.** No shorting, so bear markets can only be sat out, not
  profited from.
- **All-in sizing.** Every entry deploys 100% of available cash. No position
  sizing, no risk budgeting, no partial fills.
- **No slippage model.** `--fees` absorbs it approximately. Fine for daily
  bars and a small account; inadequate for minute bars or size.
- **`f64` for money.** Correct enough for a backtest; real systems use
  fixed-point or decimal types.
- **Single asset per run.** No portfolio, no correlation, no rebalancing.
- **One train/test split**, not walk-forward validation.
- **Bar-level fills only.** Intrabar stops and limit orders are not modelled,
  though `high`/`low` are parsed and validated so they could be.

## Possible extensions

- Validate the parameter plateau on ETH and SOL (same archive URL, different
  symbol) — the cheapest real test of whether the finding generalises.
- A second strategy (RSI mean-reversion) as a foil; the `Strategy` trait
  already supports it, and `Box<dyn Strategy>` would allow sweeping across
  strategy types rather than just parameters.
- Walk-forward validation: refit on a rolling window instead of one split.
- `--json` output. `serde` is already a dependency for this purpose; add
  `serde_json` and `#[derive(Serialize)]` on `Metrics` and `Trade`.
- Equity curve plotting via `plotters`.
- Short selling — a third `Signal` variant, which the state-based signal
  model was chosen to accommodate.

## Dependencies

| Crate | Purpose |
|---|---|
| `anyhow` | Application error handling with context chains |
| `chrono` | Timestamp formatting |
| `clap` | CLI parsing (derive API) |
| `csv` | CSV reading |
| `rayon` | Data-parallel parameter sweep |
| `serde` | Reserved for `--json` output; not yet used |
