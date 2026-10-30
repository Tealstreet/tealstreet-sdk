//! Store-backed CLI waits; scheduling and disposal belong to the account scope.
use crate::placement::{n, number};
use crate::{Evaluation, failed, rows};
use serde_json::{Value, json};

pub(crate) fn duration(text: &str) -> Result<f64, String> {
    let lower = text.trim().to_ascii_lowercase();
    let (value, unit) = if let Some(n) = lower.strip_suffix("ms") {
        (n, 1.)
    } else if let Some(n) = lower.strip_suffix('s') {
        (n, 1000.)
    } else if let Some(n) = lower.strip_suffix('m') {
        (n, 60000.)
    } else {
        (lower.as_str(), 1000.)
    };
    let value = value.trim_end();
    let ms = number(value)
        .filter(|_| !value.starts_with('.') && !value.ends_with('.'))
        .ok_or_else(|| {
            format!("sleep: invalid duration \"{text}\" (expected e.g. 250ms, 5s, 2m)")
        })?
        * unit;
    if !ms.is_finite() || ms > 600000. {
        return Err(format!("sleep: {text} exceeds 10 minute cap"));
    }
    Ok(ms)
}
#[derive(Debug)]
pub(crate) enum Wait {
    Sleep(f64),
    Clock(u32),
    Condition {
        condition: Condition,
        describe: String,
        timeout: f64,
        sustain: f64,
        symbol: String,
    },
}
impl Wait {
    pub(crate) fn parse(symbol: &str, args: &[String]) -> Result<Self, String> {
        if args.first().is_some_and(|s| s == "sleep") {
            return duration(args.get(1).ok_or("Missing duration")?).map(Self::Sleep);
        }
        let mut timeout = 30000.;
        let mut sustain = 0.;
        let mut rest = Vec::new();
        for arg in args.iter().skip(1) {
            let lower = arg.to_ascii_lowercase();
            if let Some(text) = lower.strip_prefix("timeout=") {
                timeout = duration(text).map_err(|_| "Invalid timeout")?;
                if timeout <= 0. {
                    return Err("Invalid timeout".into());
                }
            } else if let Some(text) = lower.strip_prefix("for=") {
                sustain = duration(text).map_err(|_| "Invalid for")?;
                if sustain <= 0. {
                    return Err("Invalid for".into());
                }
            } else {
                rest.push(arg.clone());
            }
        }
        if rest.is_empty() {
            return Err("Missing condition".into());
        }
        if rest.len() == 1 && rest[0].starts_with(|c: char| c.is_ascii_digit()) {
            return duration(&rest[0]).map(Self::Sleep);
        }
        if rest[0] == "time" {
            let time = rest.get(1).ok_or("Invalid time")?;
            if time.len() != 4 || !time.bytes().all(|b| b.is_ascii_digit()) {
                return Err("Invalid time".into());
            }
            let hour = time[..2].parse::<u32>().unwrap();
            let minute = time[2..].parse::<u32>().unwrap();
            if hour > 23 || minute > 59 {
                return Err("Invalid time".into());
            }
            return Ok(Self::Clock((hour * 60 + minute) * 60000));
        }
        let describe = rest.join(" ");
        let condition = Condition::parse(&describe, 0)?;
        Ok(Self::Condition {
            condition,
            describe,
            timeout,
            sustain,
            symbol: symbol.into(),
        })
    }
}
pub(crate) fn evaluate(symbol: &str, args: &[String]) -> Evaluation {
    match Wait::parse(symbol, args) {
        Err(error) => failed(error),
        Ok(_) => Evaluation::Call {
            method: "execWait",
            args: json!([symbol, args]),
            success: String::new(),
        },
    }
}
#[derive(Debug)]
pub(crate) enum Condition {
    Position {
        symbol: Option<String>,
        side: Option<String>,
        any: bool,
    },
    Compare {
        kind: String,
        op: String,
        value: f64,
        symbol: Option<String>,
    },
    Fill(Option<String>),
    Either(Vec<Self>),
    Both(Vec<Self>),
}
fn signed(text: &str) -> Option<f64> {
    let raw = text.strip_prefix(['+', '-']).unwrap_or(text);
    number(raw)
        .filter(|_| !raw.ends_with('.'))
        .map(|n| if text.starts_with('-') { -n } else { n })
}
impl Condition {
    pub(crate) fn parse(text: &str, depth: usize) -> Result<Self, String> {
        if depth > 16 {
            return Err("Condition nesting exceeds native limit".into());
        }
        let text = text.trim();
        let args = text.split_whitespace().collect::<Vec<_>>();
        let head = args
            .first()
            .ok_or("Invalid condition")?
            .to_ascii_lowercase();
        if head == "either" || head == "both" {
            let sep = if head == "either" { "or" } else { "and" };
            let mut rest = text[head.len()..].trim();
            let mut conditions = Vec::new();
            while !rest.is_empty() {
                if !rest.starts_with('(') {
                    return Err("Invalid condition".into());
                }
                let mut level = 0;
                let mut end = None;
                for (i, c) in rest.char_indices() {
                    if c == '(' {
                        level += 1;
                    } else if c == ')' {
                        level -= 1;
                        if level == 0 {
                            end = Some(i);
                            break;
                        }
                    }
                }
                let end = end.ok_or("Invalid condition")?;
                conditions.push(Self::parse(&rest[1..end], depth + 1)?);
                rest = rest[end + 1..].trim();
                if rest.is_empty() {
                    break;
                }
                let (word, tail) = rest
                    .split_once(char::is_whitespace)
                    .ok_or("Invalid condition")?;
                if !word.eq_ignore_ascii_case(sep) || tail.trim().is_empty() {
                    return Err("Invalid condition".into());
                }
                rest = tail.trim();
            }
            if conditions.len() < 2 {
                return Err("Invalid condition".into());
            }
            return Ok(if head == "either" {
                Self::Either(conditions)
            } else {
                Self::Both(conditions)
            });
        }
        if text.contains('@') {
            return Err(
                "condition: variable needs the PreprocessStore — caller must pass ctx.store".into(),
            );
        }
        let explicit = |index: usize| args.get(index).map(|s| s.to_uppercase());
        match head.as_str() {
            "position" | "long" | "short" | "anyposition" => Ok(Self::Position {
                symbol: explicit(1),
                side: if head == "long" || head == "short" {
                    Some(head.clone())
                } else {
                    None
                },
                any: head == "anyposition",
            }),
            "fill" => Ok(Self::Fill(args.get(1).map(|s| s.to_string()))),
            "price" | "mark" | "index" | "pnl" | "fundingrate" | "premium" => {
                let op = args
                    .get(1)
                    .filter(|s| matches!(**s, ">" | "<" | ">=" | "<=" | "==" | "!="))
                    .ok_or("Invalid condition")?;
                let value = args
                    .get(2)
                    .and_then(|s| signed(s))
                    .ok_or("Invalid condition")?;
                Ok(Self::Compare {
                    kind: head,
                    op: (*op).into(),
                    value,
                    symbol: explicit(3),
                })
            }
            _ => Err("Invalid condition".into()),
        }
    }
    pub(crate) fn check(&self, store: &Value, focused: &str) -> bool {
        match self {
            Self::Either(sub) => sub.iter().any(|s| s.check(store, focused)),
            Self::Both(sub) => sub.iter().all(|s| s.check(store, focused)),
            Self::Position { symbol, side, any } => rows(store, "positions").iter().any(|p| {
                n(p, "contracts") != 0.
                    && (*any || p["symbol"] == symbol.as_deref().unwrap_or(focused))
                    && side.as_ref().is_none_or(|s| p["side"] == *s)
            }),
            Self::Fill(arg) => {
                let target = arg.as_deref().unwrap_or(focused);
                let upper = target.to_uppercase();
                let by_symbol =
                    arg.is_none() || rows(store, "markets").iter().any(|m| m["symbol"] == upper);
                let pieces = target
                    .split(',')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .collect::<Vec<_>>();
                let by_id = !by_symbol
                    && !pieces.is_empty()
                    && pieces.iter().all(|s| {
                        s.len() >= 3
                            && s.bytes()
                                .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                    });
                rows(store, "orders").iter().all(|o| {
                    if !matches!(
                        o["status"].as_str(),
                        Some("open" | "creating" | "updating" | "partial")
                    ) {
                        return true;
                    }
                    if by_id {
                        !pieces.iter().any(|p| {
                            o["id"]
                                .as_str()
                                .is_some_and(|id| id == *p || id.ends_with(p))
                        })
                    } else {
                        o["symbol"] != upper
                    }
                })
            }
            Self::Compare {
                kind,
                op,
                value,
                symbol,
            } => {
                let symbol = symbol.as_deref().unwrap_or(focused);
                let actual = if kind == "pnl" {
                    let positions = rows(store, "positions")
                        .iter()
                        .filter(|p| p["symbol"] == symbol && n(p, "contracts") != 0.)
                        .collect::<Vec<_>>();
                    if positions.is_empty() {
                        return false;
                    }
                    Some(positions.iter().map(|p| n(p, "unrealizedPnl")).sum())
                } else {
                    let Some(t) = rows(store, "tickers")
                        .iter()
                        .find(|t| t["symbol"] == symbol)
                    else {
                        return false;
                    };
                    match kind.as_str() {
                        "price" => {
                            let side = match op.as_str() {
                                ">" | ">=" => "ask",
                                "<" | "<=" => "bid",
                                _ => "last",
                            };
                            t[side].as_f64().or_else(|| t["last"].as_f64())
                        }
                        "fundingrate" => t["fundingRate"].as_f64(),
                        "premium" => t["mark"]
                            .as_f64()
                            .zip(t["index"].as_f64())
                            .map(|(a, b)| a - b),
                        key => t[key].as_f64(),
                    }
                };
                let Some(actual) = actual.filter(|n| n.is_finite()) else {
                    return false;
                };
                match op.as_str() {
                    ">" => actual > *value,
                    "<" => actual < *value,
                    ">=" => actual >= *value,
                    "<=" => actual <= *value,
                    "==" => actual == *value,
                    "!=" => actual != *value,
                    _ => false,
                }
            }
        }
    }
}
#[cfg(not(target_arch = "wasm32"))]
pub(crate) async fn run(
    wait: Wait,
    cx: &safe_cex_algos::AlgoCx,
    snapshot: impl Fn() -> Value,
) -> Result<Value, Value> {
    match wait {
        Wait::Sleep(ms) => cx.sleep_ms(ms.ceil() as u64).await,
        Wait::Clock(target) => {
            let now = cx.now_ms().rem_euclid(86400000.);
            let mut delay = f64::from(target) - now;
            if delay <= 0. {
                delay += 86400000.;
            }
            cx.sleep_ms(delay.ceil() as u64).await;
        }
        Wait::Condition {
            condition,
            describe,
            timeout,
            sustain,
            symbol,
        } => {
            let start = safe_cex_algos::executor::Instant::now();
            let mut held = None;
            loop {
                let elapsed = start.elapsed().as_secs_f64() * 1000.;
                if condition.check(&snapshot(), &symbol) {
                    let since = *held.get_or_insert(elapsed);
                    if elapsed - since >= sustain {
                        break;
                    }
                } else {
                    held = None;
                }
                if elapsed >= timeout {
                    return Err(
                        json!({"message":format!("wait timed out ({timeout}ms): {describe}{}",if sustain>0. {format!(" (sustain {sustain}ms)")}else{String::new()}),"outcome":"notSent"}),
                    );
                }
                cx.sleep_ms(100).await;
            }
        }
    }
    Ok(Value::Null)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn sleep_duration_matches_typescript_units_cap_and_strict_shape() {
        for (text, ms) in [
            ("5", 5000.),
            ("250ms", 250.),
            ("30s", 30000.),
            ("2m", 120000.),
            ("0", 0.),
            ("0.5s", 500.),
            ("10m", 600000.),
        ] {
            assert_eq!(duration(text).unwrap(), ms);
        }
        for text in ["", "-1s", ".5s", "5.", "1h", "1e3", "10mX", "600001ms"] {
            assert!(duration(text).is_err(), "{text}");
        }
    }
    fn store() -> Value {
        json!({"markets":[{"symbol":"BTCUSDT"}],"tickers":[{"symbol":"BTCUSDT","last":100.,"bid":99.,"ask":101.,"mark":102.,"index":98.,"fundingRate":0.001}],"positions":[{"symbol":"BTCUSDT","side":"long","contracts":1,"unrealizedPnl":5},{"symbol":"BTCUSDT","side":"short","contracts":2,"unrealizedPnl":-1}],"orders":[{"id":"order-abc","symbol":"BTCUSDT","status":"open"},{"id":"order-def","symbol":"ETHUSDT","status":"partial"}]})
    }
    fn check(text: &str, store: &Value) -> bool {
        Condition::parse(text, 0).unwrap().check(store, "BTCUSDT")
    }
    #[test]
    fn store_conditions_cover_side_specific_prices_hedges_funding_and_premium() {
        let s = store();
        for c in [
            "position",
            "long",
            "short",
            "anyposition",
            "price > 100",
            "price < 100",
            "price == 100",
            "mark == 102",
            "index == 98",
            "pnl == 4",
            "fundingrate > .0001",
            "premium == 4",
        ] {
            assert!(check(c, &s), "{c}");
        }
        for c in [
            "position ETHUSDT",
            "price > 101",
            "price < 99",
            "pnl > 4",
            "mark == 100",
        ] {
            assert!(!check(c, &s), "{c}");
        }
        let mut s = s;
        s["tickers"][0]["ask"] = Value::Null;
        assert!(!check("price > 100", &s));
        s["tickers"][0]["bid"] = json!(0.);
        assert!(
            check("price < 1", &s),
            "finite zero bid remains a value like TS"
        );
    }
    #[test]
    fn fill_wait_tracks_live_statuses_and_real_id_suffixes_without_assuming_fills() {
        let mut s = store();
        assert!(!check("fill", &s));
        assert!(!check("fill abc,def", &s));
        s["orders"][0]["status"] = json!("canceled");
        assert!(check("fill BTCUSDT", &s));
        assert!(check("fill abc", &s));
        assert!(!check("fill abc,def", &s));
        s["orders"][1]["status"] = json!("closed");
        assert!(check("fill abc,def", &s));
        for status in ["open", "creating", "updating", "partial"] {
            s["orders"][0]["status"] = json!(status);
            assert!(!check("fill abc", &s));
        }
    }
    #[test]
    fn nested_combinators_and_invalid_or_host_only_conditions_are_explicit() {
        assert!(check(
            "either (both (long) and (price > 100)) or (pnl < -5)",
            &store()
        ));
        assert!(!check("both (long) and (price > 200)", &store()));
        for text in [
            "either (long)",
            "both (long) or (short)",
            "either (long) or",
            "either (long",
            "price > 123abc",
            "price > @stop",
            "@pnl > 3",
            "wat",
        ] {
            assert!(Condition::parse(text, 0).is_err(), "{text}");
        }
        let nested = format!("{}long{}", "either (".repeat(18), ") or (long)".repeat(18));
        assert!(Condition::parse(&nested, 0).is_err());
    }
}
