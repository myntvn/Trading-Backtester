use crate::{data::fmt_date, engine::Trade, metrics::Metrics};

const LABEL_W: usize = 16;
const COL_W: usize = 18;

fn fmt_opt(v: Option<f64>, suffix: &str) -> String {
    match v {
        Some(x) => format!("{x:.2}{suffix}"),
        None => "n/a".to_string(),
    }
}

/// How to render one row of the comparison table.
///
/// This is a function *pointer*, not a closure type. The lambdas below
/// capture nothing from their environment, so they coerce to it — which is
/// what lets the whole table live in a `const`.
type Cell = fn(&Metrics) -> String;

const ROW: &[(&str, Cell)] = &[
    ("final equity", |m| format!("{:.2}", m.final_equity)),
    ("pnl", |m| format!("{:.2}", m.pnl)),
    ("return", |m| format!("{:.2}%", m.total_return_pct)),
    ("sharpe", |m| fmt_opt(m.sharpe, "")),
    ("max drawdown", |m| format!("{:.2}%", m.max_drawdown_pct)),
    ("exposure", |m| format!("{:.1}%", m.exposure_pct)),
    ("trades", |m| m.trades.to_string()),
    ("wins / losses", |m| format!("{} / {}", m.wins, m.losses)),
    ("win rate", |m| fmt_opt(m.win_rate_pct, "%")),
    ("avg win", |m| fmt_opt(m.avg_win, "")),
    ("avg loss", |m| fmt_opt(m.avg_loss, "")),
    ("profit factor", |m| fmt_opt(m.profit_factor, "")),
    ("fees paid", |m| format!("{:.2}", m.fees_paid)),
];

pub fn print_header(bars: usize, first_ts: i64, last_ts: i64, fee_bps: f64) {
    println!(
        "{} bars   {} -> {}   {:.1} bps fees",
        bars,
        fmt_date(first_ts),
        fmt_date(last_ts),
        fee_bps
    );
}

pub fn print_trades(trades: &[Trade]) {
    println!("\ntrade history");

    if trades.is_empty() {
        println!("  (no trade)");
        return;
    }

    println!(
        "{:<12} {:<12} {:>10} {:>10} {:>11} {:>8}",
        "entry", "exit", "in", "out", "pnl", "ret%"
    );

    for t in trades {
        println!(
            "{:<12} {:<12} {:>10.2} {:>10.2} {:>11.2} {:>7.2}%{}",
            fmt_date(t.entry_ts),
            fmt_date(t.exit_ts),
            t.entry_price,
            t.exit_price,
            t.pnl,
            t.return_pct(),
            if t.forced_exit { "  *still open" } else { "" },
        );
    }
}

/// Print one column per `(name, metrics)` pair. Works with any number of
/// columns, so `--no-benchmark` is just a shorter slice.
pub fn print_comparison(columns: &[(&str, &Metrics)]) {
    println!();

    print!("{:<LABEL_W$}", "");
    for &(name, _) in columns {
        print!("{name:>COL_W$}");
    }
    println!();

    println!("{}", "-".repeat(LABEL_W + COL_W * columns.len()));

    for &(label, cell) in ROW {
        print!("{label:<LABEL_W$}");
        for &(_, m) in columns {
            print!("{:>COL_W$}", cell(m));
        }
        println!();
    }
}
