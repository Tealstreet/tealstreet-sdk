//! CLI entry points into the account's existing typed algo instances.
use crate::placement::{Placement, n, number, tradfi_preflight};
use crate::{Evaluation, Host, failed, log, resolve, rows};
use serde_json::{Value, json};
#[derive(Clone, Debug, PartialEq)]
pub enum AlgoRequest {
    ChaseStart {
        options: Vec<Value>,
        errors: Vec<String>,
    },
    ChaseCancel {
        keep: bool,
    },
    ChasersList,
    ChasersCancel(String),
    TwapStart(Value),
    TwapList,
    TwapStop(String),
}
#[derive(Clone, Debug, Default)]
pub struct AlgoReply {
    pub message: String,
    pub logs: Vec<String>,
}
pub(crate) fn evaluate(
    symbol: &str,
    args: &[String],
    store: &Value,
    req_id: &Option<Value>,
    host: &mut impl Host,
) -> Evaluation {
    let request = match args[0].as_str() {
        "chase" => chase(symbol, args, store),
        "chasers" => Ok(if args.get(1).is_some_and(|s| s == "cancel") {
            AlgoRequest::ChasersCancel(args.get(2).cloned().unwrap_or_default())
        } else {
            AlgoRequest::ChasersList
        }),
        "twap" => twap(symbol, args, store),
        _ => unreachable!(),
    };
    match request {
        Err(error) => failed(error),
        Ok(request) => match host.algo(request) {
            Ok(reply) => {
                for message in reply.logs {
                    log(host, req_id, &message);
                }
                Evaluation::Text {
                    message: reply.message,
                    result: json!({"handled":true,"success":true}),
                }
            }
            Err(error) => {
                let message = error["message"]
                    .as_str()
                    .unwrap_or("Algorithm refused")
                    .to_string();
                log(host, req_id, &format!("Error: {message}"));
                Evaluation::Done(
                    json!({"handled":true,"success":false,"error":message,"errorReported":true,"errorDetails":error}),
                )
            }
        },
    }
}
fn health(store: &Value, reduce: bool) -> Result<(), String> {
    if !reduce
        && store["socketHealth"]["privateInitialized"] == true
        && store["socketHealth"]["privateHealthy"] == false
    {
        return Err("Account connection is degraded (private data stream is stale) — refusing to open or increase exposure. Wait for reconnect (run `health`); close / cancel / reduce-only orders stay allowed.".into());
    }
    Ok(())
}
fn size(
    store: &Value,
    symbol: &str,
    side: &'static str,
    text: &str,
    reduce: bool,
) -> Result<f64, String> {
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
        .ok_or_else(|| format!("Market not found for {symbol}"))?;
    tradfi_preflight(store, market)?;
    let last = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .map(|t| n(t, "last"))
        .unwrap_or(0.);
    let lot = market["precision"]["amount"]
        .as_f64()
        .filter(|n| *n != 0.)
        .unwrap_or(1e-8);
    let amount = Placement::sized(symbol, side, text, reduce).size(store, market, last, lot)?;
    if amount <= 0. || !amount.is_finite() {
        return Err("Size resolved to zero — algorithm not started".into());
    }
    Ok(amount)
}
fn chase(symbol: &str, args: &[String], store: &Value) -> Result<AlgoRequest, String> {
    let side = args
        .get(1)
        .ok_or("Please specify a side: chase buy or chase sell")?;
    if side == "cancel" {
        return Ok(AlgoRequest::ChaseCancel {
            keep: args.get(2).is_some_and(|s| s == "keep"),
        });
    }
    if !matches!(side.as_str(), "buy" | "sell") {
        return Err(format!("Invalid side \"{side}\". Must be 'buy' or 'sell'."));
    }
    let side = if side == "buy" { "buy" } else { "sell" };
    let reduce = args.iter().any(|s| s == "reduce");
    health(store, reduce)?;
    let mut targets = Vec::new();
    let mut size_text = None;
    let mut terminate = None;
    let mut index = 2;
    while let Some(token) = args.get(index) {
        if token == "reduce" {
            index += 1;
            continue;
        }
        if token == "to" {
            let text = args
                .get(index + 1)
                .ok_or("chase: 'to' must be followed by a length (price / % / $)")?;
            let (kind, raw) = if let Some(n) = text.strip_suffix('%') {
                ("percent", n)
            } else if let Some(n) = text.strip_prefix('$').or_else(|| text.strip_suffix('$')) {
                ("usd", n)
            } else {
                ("price", text.as_str())
            };
            let value = number(raw).filter(|n| *n > 0.).ok_or_else(|| {
                format!("chase: invalid 'to' length '{text}' — expected price, NN%, $NN, or NN$")
            })?;
            terminate = Some((kind, value));
            index += 2;
            continue;
        }
        if token.contains('$') || token.contains('%') {
            if size_text.is_some() {
                return Err("Unexpected chase size".into());
            }
            size_text = Some(token.clone());
        } else {
            for part in token.split(',') {
                targets.push(
                    resolve(store, part).ok_or_else(|| format!("Unknown symbol \"{part}\""))?,
                );
            }
        }
        index += 1;
    }
    let text = size_text.ok_or("Size argument missing. Usage: chase buy $amount")?;
    if targets.is_empty() {
        targets.push(symbol.into());
    }
    let mut options = Vec::new();
    let mut errors = Vec::new();
    for symbol in targets {
        let amount = match size(store, &symbol, side, &text, reduce) {
            Ok(amount) => amount,
            Err(error) => {
                errors.push(format!("{symbol}: {error}"));
                continue;
            }
        };
        let mut option = json!({"symbol":symbol,"side":side,"size":amount,"reduceOnly":reduce,"maxAttempts":1000,"origin":"cli"});
        if let Some((kind, value)) = terminate {
            let anchor = rows(store, "tickers")
                .iter()
                .find(|t| t["symbol"] == symbol)
                .map(|t| n(t, "last"))
                .filter(|p| *p > 0.)
                .ok_or_else(|| {
                    format!("No anchor price available for {symbol} — can't compute 'to' threshold")
                })?;
            option["terminate"] = json!({"type":kind,"value":value,"anchorPrice":anchor});
        }
        options.push(option);
    }
    Ok(AlgoRequest::ChaseStart { options, errors })
}
fn duration(text: &str) -> Result<f64, String> {
    let mut digits = String::new();
    let mut total = 0.;
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
        } else {
            let value = number(&digits).ok_or("Invalid duration format")?;
            digits.clear();
            total += value
                * match c.to_ascii_lowercase() {
                    'h' => 60.,
                    'm' => 1.,
                    's' => 1. / 60.,
                    _ => return Err("Invalid duration format".into()),
                };
        }
    }
    if !digits.is_empty() || total <= 0. || !total.is_finite() {
        return Err(format!(
            "Invalid duration format: {text}. Use formats like: 30s, 5m, 1h, 1h30m"
        ));
    }
    Ok(total)
}
fn twap(symbol: &str, args: &[String], store: &Value) -> Result<AlgoRequest, String> {
    let head = args.get(1).map(String::as_str).unwrap_or("");
    if head == "list" {
        return Ok(AlgoRequest::TwapList);
    }
    if head == "stop" {
        return Ok(AlgoRequest::TwapStop(args.get(2).cloned().ok_or(
            "Please specify a TWAP ID to stop or \"all\" to stop all",
        )?));
    }
    let chase = head == "chase";
    let side_index = if chase { 2 } else { 1 };
    let side = args
        .get(side_index)
        .filter(|s| matches!(s.as_str(), "buy" | "sell"))
        .ok_or("Unknown TWAP side; use buy or sell")?;
    let side = if side == "buy" { "buy" } else { "sell" };
    let mut index = side_index + 1;
    let mut target = symbol.to_string();
    let mut size_text = None;
    let mut minutes = None;
    let mut count = 10u32;
    let mut reduce = false;
    let mut use_chaser = chase;
    let mut timeout_pct = 50.;
    let mut timeout_action = "market";
    let mut fail_action = "market";
    while let Some(token) = args.get(index) {
        match token.as_str() {
            "into" => {
                count = args
                    .get(index + 1)
                    .and_then(|s| s.parse::<u32>().ok())
                    .filter(|n| *n > 0)
                    .ok_or("twap: 'into' must be followed by a positive integer slice count")?;
                index += 2;
                continue;
            }
            "over" => {
                minutes = Some(
                    args.get(index + 1)
                        .and_then(|s| number(s))
                        .filter(|m| *m > 0.)
                        .ok_or("twap: 'over' must be followed by a positive number of minutes")?,
                );
                index += 2;
                continue;
            }
            "reduce" => reduce = true,
            "chaser" => use_chaser = true,
            "timeout" if chase => {
                timeout_pct = args
                    .get(index + 1)
                    .and_then(|s| s.strip_suffix('%'))
                    .and_then(number)
                    .filter(|p| *p > 0. && *p <= 100.)
                    .ok_or("twap chase timeout: percent must be in (0,100]")?;
                timeout_action = match args.get(index + 2).map(String::as_str) {
                    Some("market") => "market",
                    Some("next") => "next",
                    _ => return Err("twap chase timeout: action must be 'market' or 'next'".into()),
                };
                index += 3;
                continue;
            }
            "fail" if chase => {
                fail_action = match args.get(index + 1).map(String::as_str) {
                    Some("market") => "market",
                    Some("next") => "next",
                    _ => return Err("twap chase fail: action must be 'market' or 'next'".into()),
                };
                index += 2;
                continue;
            }
            _ => {
                if let Some(text) = token.strip_prefix("duration=") {
                    minutes = Some(duration(text)?);
                } else if let Some(text) = token.strip_prefix("slices=") {
                    count = text
                        .parse::<u32>()
                        .ok()
                        .filter(|n| *n > 0)
                        .ok_or("Invalid slice count")?;
                } else if token.starts_with('$') || token.ends_with('$') {
                    if size_text.is_some() {
                        return Err("Unexpected TWAP size".into());
                    }
                    size_text = Some(token.clone());
                } else {
                    target = resolve(store, token)
                        .ok_or_else(|| format!("Unknown symbol \"{token}\""))?;
                }
            }
        }
        index += 1;
    }
    let minutes =
        minutes.ok_or("Duration is required. Usage: twap buy $100 duration=1h slices=10")?;
    let text = size_text
        .ok_or("Size must be in USD format (e.g., $100). Usage: twap buy $100 duration=1h")?;
    health(store, reduce)?;
    let amount = size(store, &target, side, &text, false)?;
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == target)
        .ok_or_else(|| format!("Market not found for symbol {target}"))?;
    let mut options = json!({"activeMarket":market,"size":amount,"duration":minutes,"ordersCount":count,"side":side,"reduceOnly":reduce,"useChaserOrders":use_chaser,"origin":"cli"});
    if chase {
        options["timeoutPct"] = json!(timeout_pct);
        options["timeoutAction"] = json!(timeout_action);
        options["failAction"] = json!(fail_action);
    }
    Ok(AlgoRequest::TwapStart(options))
}
