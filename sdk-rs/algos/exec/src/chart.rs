//! CLI candle chart: engine query followed by a bounded store-overlay projection.
use crate::display::dollars;
use crate::placement::n;
use crate::{Evaluation, failed, resolve, rows};
use serde_json::{Value, json};
const GREEN: &str = "\x1b[32m";
const RED: &str = "\x1b[31m";
const DIM: &str = "\x1b[2m";
const CYAN: &str = "\x1b[36m";
const RESET: &str = "\x1b[0m";
const FRAMES: &[(&str, u64)] = &[
    ("1m", 60000),
    ("3m", 180000),
    ("5m", 300000),
    ("15m", 900000),
    ("30m", 1800000),
    ("1h", 3600000),
    ("2h", 7200000),
    ("4h", 14400000),
    ("6h", 21600000),
    ("12h", 43200000),
    ("1d", 86400000),
    ("1w", 604800000),
];
pub(crate) fn evaluate(symbol: &str, args: &[String], store: &Value) -> Evaluation {
    let mut symbol = symbol.to_string();
    let mut interval = "1h";
    let mut limit = 30;
    for arg in args.iter().skip(1) {
        if let Some((frame, _)) = FRAMES
            .iter()
            .find(|(frame, _)| arg.eq_ignore_ascii_case(frame))
        {
            interval = frame;
        } else if arg.bytes().all(|b| b.is_ascii_digit()) {
            if let Some(value) = arg.parse::<u32>().ok().filter(|n| *n > 0 && *n <= 100) {
                limit = value;
            }
        } else {
            let Some(target) = resolve(store, arg) else {
                return failed(format!("Unknown symbol: {arg}"));
            };
            symbol = target;
        }
    }
    if !rows(store, "markets").iter().any(|m| m["symbol"] == symbol) {
        return failed(format!("Unknown symbol: {symbol}"));
    }
    Evaluation::Call {
        method: "fetchOHLCV",
        args: json!([{"symbol":symbol,"interval":interval,"limit":limit}]),
        success: String::new(),
    }
}
struct Line {
    price: f64,
    color: &'static str,
    label: String,
}
fn trimmed(value: f64) -> String {
    let s = format!("{:.4}", value.abs());
    s.trim_end_matches('0').trim_end_matches('.').into()
}
fn pnl(value: f64) -> String {
    format!("{}${:.2}", if value >= 0. { "+" } else { "-" }, value.abs())
}
fn countdown(ms: f64) -> String {
    let seconds = (ms / 1000.).ceil() as u64;
    let days = seconds / 86400;
    let hours = seconds % 86400 / 3600;
    let minutes = seconds % 3600 / 60;
    let sec = seconds % 60;
    if days > 0 {
        format!("{days}d {hours}h")
    } else if hours > 0 {
        format!("{hours}h {minutes}m")
    } else if minutes > 0 {
        format!("{minutes}m {sec}s")
    } else {
        format!("{sec}s")
    }
}
pub(crate) fn render(
    candles: &Value,
    query: &Value,
    store: &Value,
    now: f64,
) -> Result<String, String> {
    let symbol = query["symbol"].as_str().ok_or("Invalid chart symbol")?;
    let interval = query["interval"].as_str().ok_or("Invalid chart interval")?;
    let candles = candles.as_array().ok_or("Invalid candle result")?;
    if candles.is_empty() {
        return Err(format!("No candle data available for {symbol}"));
    }
    if candles.len() > 100 {
        return Err("Candle result exceeds native chart limit".into());
    }
    for candle in candles {
        if ["timestamp", "open", "high", "low", "close"]
            .iter()
            .any(|key| candle[*key].as_f64().is_none_or(|n| !n.is_finite()))
            || n(candle, "low") > n(candle, "high")
            || n(candle, "open") < n(candle, "low")
            || n(candle, "open") > n(candle, "high")
            || n(candle, "close") < n(candle, "low")
            || n(candle, "close") > n(candle, "high")
        {
            return Err("Invalid candle result".into());
        }
    }
    let precision = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
        .and_then(|m| m["precision"]["price"].as_f64());
    let last_candle = candles.last().unwrap();
    let last = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .and_then(|t| t["last"].as_f64())
        .unwrap_or(n(last_candle, "close"));
    let mut overlays = Vec::new();
    let mut summary = None;
    if let Some(position) = rows(store, "positions")
        .iter()
        .find(|p| p["symbol"] == symbol)
        && n(position, "contracts") != 0.
    {
        let side = if position["side"] == "long" {
            "Long"
        } else {
            "Short"
        };
        let color = if side == "Long" { GREEN } else { RED };
        let size = trimmed(n(position, "contracts"));
        let profit = pnl(n(position, "unrealizedPnl"));
        overlays.push(Line {
            price: n(position, "entryPrice"),
            color,
            label: format!("{side} {size} {profit}"),
        });
        summary = Some(format!(
            "{color}{side}{RESET} {size} @ {} | PnL: {}{profit}{RESET}",
            dollars(n(position, "entryPrice"), precision),
            if n(position, "unrealizedPnl") >= 0. {
                GREEN
            } else {
                RED
            }
        ));
    }
    for order in rows(store, "orders")
        .iter()
        .filter(|o| o["symbol"] == symbol && o["status"] == "open")
    {
        let (label, price, color) = match order["type"].as_str().unwrap_or("") {
            "market" => continue,
            "stop_market" => (
                "SL",
                order["stopLoss"]
                    .as_f64()
                    .filter(|p| *p > 0.)
                    .unwrap_or(n(order, "price")),
                RED,
            ),
            "take_profit_market" => (
                "TP",
                order["takeProfit"]
                    .as_f64()
                    .filter(|p| *p > 0.)
                    .unwrap_or(n(order, "price")),
                GREEN,
            ),
            "trailing_stop_market" => ("TSL", n(order, "price"), RED),
            _ => ("Limit", n(order, "price"), CYAN),
        };
        if price > 0. {
            overlays.push(Line {
                price,
                color,
                label: format!("{label} {}", trimmed(n(order, "amount"))),
            });
        }
    }
    let minimum = candles
        .iter()
        .map(|c| n(c, "low"))
        .chain(std::iter::once(last))
        .chain(overlays.iter().map(|l| l.price))
        .fold(f64::INFINITY, f64::min);
    let maximum = candles
        .iter()
        .map(|c| n(c, "high"))
        .chain(std::iter::once(last))
        .chain(overlays.iter().map(|l| l.price))
        .fold(f64::NEG_INFINITY, f64::max);
    let first = n(&candles[0], "close");
    let change = if first != 0. {
        (last - first) / first * 100.
    } else {
        0.
    };
    let sign = if change >= 0. { "+" } else { "" };
    let color = if change >= 0. { GREEN } else { RED };
    let remaining = FRAMES
        .iter()
        .find(|(frame, _)| *frame == interval)
        .map(|(_, ms)| n(last_candle, "timestamp") * 1000. + *ms as f64 - now)
        .filter(|ms| *ms > 0.)
        .map(countdown);
    let mut out = format!(
        "{symbol} {interval} ({} candles) | Last: {} {color}{sign}{change:.2}%{RESET}",
        candles.len(),
        dollars(last, precision)
    );
    if let Some(countdown) = &remaining {
        out.push_str(&format!(" | {DIM}closes in {CYAN}{countdown}{RESET}"));
    }
    if let Some(summary) = summary {
        out.push('\n');
        out.push_str(&summary);
    }
    out.push_str("\n\n");
    if maximum == minimum {
        out.push_str("Price range is zero - cannot render chart");
        return Ok(out);
    }
    let row_of = |price: f64| {
        ((maximum - price) / (maximum - minimum) * 14.)
            .floor()
            .clamp(0., 14.) as usize
    };
    let mut grid = vec![vec![None; candles.len()]; 15];
    for (col, candle) in candles.iter().enumerate() {
        let color = if n(candle, "close") >= n(candle, "open") {
            GREEN
        } else {
            RED
        };
        let open = row_of(n(candle, "open"));
        let close = row_of(n(candle, "close"));
        for (row, cells) in grid
            .iter_mut()
            .enumerate()
            .take(row_of(n(candle, "low")) + 1)
            .skip(row_of(n(candle, "high")))
        {
            cells[col] = Some((
                color,
                if row >= open.min(close) && row <= open.max(close) {
                    '█'
                } else {
                    '│'
                },
            ));
        }
    }
    let width = dollars(maximum, precision).chars().count();
    let lastrow = row_of(last);
    for (row, cells) in grid.iter().enumerate() {
        if row == 0 || row == 7 || row == 14 {
            let label = dollars(maximum - row as f64 / 14. * (maximum - minimum), precision);
            out.push_str(&format!("{DIM}{label:>width$}{RESET} ┤"));
        } else {
            out.push_str(&format!("{} │", " ".repeat(width)));
        }
        let overlay = overlays.iter().find(|l| row_of(l.price) == row);
        for cell in cells {
            if let Some((color, c)) = cell {
                out.push_str(&format!("{color}{c}{RESET} "));
            } else if let Some(line) = overlay {
                out.push_str(&format!("{}·{RESET} ", line.color));
            } else {
                out.push_str("  ");
            }
        }
        if let Some(line) = overlay {
            out.push_str(&format!("{}◄ {}{RESET}", line.color, line.label));
        } else if row == lastrow {
            out.push_str(&format!(
                "{}◄ {}{RESET}",
                if n(last_candle, "close") >= n(last_candle, "open") {
                    GREEN
                } else {
                    RED
                },
                dollars(last, precision)
            ));
        } else if row == lastrow + 1
            && let Some(countdown) = &remaining
        {
            out.push_str(&format!("  {countdown}"));
        }
        out.push('\n');
    }
    out.push_str(&format!(
        "{} └{}",
        " ".repeat(width),
        "─".repeat(candles.len() * 2)
    ));
    if candles.len() >= 3 {
        let clock = |timestamp: f64| {
            let minutes = (timestamp.rem_euclid(86400.) / 60.) as u32;
            format!("{:02}:{:02}", minutes / 60, minutes % 60)
        };
        out.push_str(&format!(
            "\n{}{DIM}{}{}{} UTC{RESET}",
            " ".repeat(width + 2),
            clock(n(&candles[0], "timestamp")),
            " ".repeat((candles.len() * 2).saturating_sub(10)),
            clock(n(last_candle, "timestamp"))
        ));
    }
    Ok(out)
}

#[cfg(test)]
mod tests {
    use super::*;
    fn candles() -> Value {
        json!([{"timestamp":0,"open":100,"high":110,"low":90,"close":105},{"timestamp":60,"open":105,"high":108,"low":95,"close":98},{"timestamp":120,"open":98,"high":109,"low":97,"close":108}])
    }
    fn query() -> Value {
        json!({"symbol":"BTCUSDT","interval":"1m","limit":30})
    }
    fn store() -> Value {
        json!({"markets":[{"symbol":"BTCUSDT","precision":{"price":0.1}}],"tickers":[{"symbol":"BTCUSDT","last":108}],"positions":[{"symbol":"BTCUSDT","side":"long","contracts":0.25,"entryPrice":100,"unrealizedPnl":2}],"orders":[{"symbol":"BTCUSDT","type":"stop_market","side":"sell","amount":0.25,"price":0,"stopLoss":92,"status":"open"},{"symbol":"BTCUSDT","type":"take_profit_market","side":"sell","amount":0.25,"price":0,"takeProfit":109,"status":"open"}]})
    }
    #[test]
    fn renders_candles_real_position_and_trigger_overlays_without_treating_them_as_limits() {
        let text = render(&candles(), &query(), &store(), 180000.).unwrap();
        for expected in [
            "BTCUSDT 1m (3 candles)",
            "Long",
            "0.25",
            "PnL:",
            "SL 0.25",
            "TP 0.25",
            "█",
            "UTC",
        ] {
            assert!(text.contains(expected), "{expected}: {text}");
        }
        assert!(!text.contains("Limit 0.25"));
    }
    #[test]
    fn empty_malformed_and_oversized_candles_refuse_without_fabricated_price_data() {
        assert!(
            render(&json!([]), &query(), &store(), 0.)
                .unwrap_err()
                .contains("No candle data")
        );
        for value in [
            Value::Null,
            json!([{"timestamp":0,"open":1,"close":2}]),
            json!([{"timestamp":0,"open":1,"close":2,"low":3,"high":2}]),
            json!(vec![candles()[0].clone(); 101]),
        ] {
            assert!(render(&value, &query(), &store(), 0.).is_err());
        }
    }
    #[test]
    fn countdown_flat_range_and_symbol_interval_admission_are_bounded() {
        let mut s = store();
        s["positions"] = json!([]);
        s["orders"] = json!([]);
        let flat = json!([{"timestamp":0,"open":108,"high":108,"low":108,"close":108}]);
        let text = render(&flat, &query(), &s, 30000.).unwrap();
        assert!(text.contains("closes in"));
        assert!(text.contains("30s"));
        assert!(text.contains("Price range is zero"));
        let Evaluation::Call { args, .. } =
            evaluate("BTCUSDT", &["chart".into(), "4h".into(), "50".into()], &s)
        else {
            panic!()
        };
        assert_eq!(args[0]["interval"], "4h");
        assert_eq!(args[0]["limit"], 50);
        let Evaluation::Done(result) = evaluate("BTCUSDT", &["chart".into(), "BTCX".into()], &s)
        else {
            panic!()
        };
        assert_eq!(result["success"], false);
    }
    #[test]
    fn intervals_match_the_typescript_chart_contract() {
        let ts = include_str!("../../../../packages/cli-core/src/commands/trading/ChartCommand.ts");
        let values = ts
            .split("export const VALID_TIMEFRAMES")
            .nth(1)
            .unwrap()
            .split("= [")
            .nth(1)
            .unwrap()
            .split("];")
            .next()
            .unwrap();
        let values = values
            .lines()
            .filter_map(|l| {
                l.trim()
                    .strip_prefix('\'')
                    .and_then(|s| s.split('\'').next())
            })
            .collect::<Vec<_>>();
        assert_eq!(values, FRAMES.iter().map(|(s, _)| *s).collect::<Vec<_>>());
    }
}
