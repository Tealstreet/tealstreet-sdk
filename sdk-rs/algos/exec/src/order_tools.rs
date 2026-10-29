//! Order selectors, price-only bump and the batch ladder allocator.
use crate::placement::{Placement, n, number, prepare_order, price};
use crate::{Action, Evaluation, failed, resolve, rows};
use serde_json::{Value, json};
const FILTERS: &[&str] = &[
    "buys", "sells", "buy", "sell", "limits", "stops", "ro", "xro", "top", "bottom", "first",
    "last", "from", "to",
];
fn target(symbol: &str, args: &[String], store: &Value) -> Result<String, String> {
    let mut target = symbol.to_string();
    for token in args.iter().skip(1) {
        if FILTERS.contains(&token.as_str()) || number(token).is_some() {
            continue;
        }
        if let Some(s) = resolve(store, token) {
            target = s;
        } else {
            return Err(format!("Unknown symbol \"{token}\""));
        }
    }
    Ok(target)
}
pub(crate) fn advanced_cancel(symbol: &str, args: &[String], store: &Value) -> Option<Evaluation> {
    let head = args.get(1)?.as_str();
    if !matches!(
        head,
        "top" | "bottom" | "first" | "last" | "from" | "buy" | "sell"
    ) {
        return None;
    }
    let result = (|| -> Result<Evaluation, String> {
        let target = target(symbol, args, store)?;
        let mut selected: Vec<_> = rows(store, "orders")
            .iter()
            .filter(|o| o["symbol"] == target)
            .cloned()
            .collect();
        let success;
        if matches!(head, "buy" | "sell") {
            if args.get(2).map(String::as_str) != Some("stops") {
                return Err("Not implemented".into());
            }
            selected.retain(|o| o["side"] == head && o["type"] != "limit" && o["type"] != "market");
            success = format!("Cancelling all {target} {head} stops");
        } else {
            selected.retain(|o| o["type"] == "limit" && o["status"] == "open");
            if head == "from" {
                let a = args
                    .get(2)
                    .and_then(|s| number(s))
                    .ok_or("Usage: cancel from <p1> to <p2> [SYMBOL]")?;
                let b = args
                    .get(4)
                    .and_then(|s| number(s))
                    .ok_or("Usage: cancel from <p1> to <p2> [SYMBOL]")?;
                if args.get(3).map(String::as_str) != Some("to") {
                    return Err("Usage: cancel from <p1> to <p2> [SYMBOL]".into());
                }
                selected.retain(|o| n(o, "price") >= a.min(b) && n(o, "price") <= a.max(b));
                success = format!(
                    "Cancelling {} {target} order(s) in [{}, {}]",
                    selected.len(),
                    a.min(b),
                    a.max(b)
                );
            } else {
                let count = args
                    .get(2)
                    .and_then(|s| s.parse::<usize>().ok())
                    .filter(|n| *n > 0)
                    .ok_or_else(|| format!("Usage: cancel {head} <n> [SYMBOL]"))?;
                let key = if matches!(head, "first" | "last") {
                    "lastUpdate"
                } else {
                    "price"
                };
                selected.sort_by(|a, b| n(a, key).total_cmp(&n(b, key)));
                if matches!(head, "top" | "last") {
                    selected.reverse();
                }
                selected.truncate(count);
                success = format!("Cancelling {} {head} {target} order(s)", selected.len());
            }
            if selected.is_empty() {
                return Ok(Evaluation::Text {
                    message: if head == "from" {
                        format!("No {target} limit orders in range")
                    } else {
                        format!("No open limit orders on {target}")
                    },
                    result: json!({"handled":true,"success":true}),
                });
            }
        }
        Ok(Evaluation::Call {
            method: "cancelOrders",
            args: json!([selected]),
            success,
        })
    })();
    Some(result.unwrap_or_else(failed))
}
pub(crate) fn evaluate(symbol: &str, args: &[String], store: &Value, raw: &str) -> Evaluation {
    match args[0].as_str() {
        "scale" => scale(symbol, args, store),
        "bump" => bump(symbol, args, store, raw),
        _ => unreachable!(),
    }
    .unwrap_or_else(failed)
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
fn scale(symbol: &str, args: &[String], store: &Value) -> Result<Evaluation, String> {
    if args.len() < 9
        || !matches!(args[1].as_str(), "buy" | "sell")
        || args[3] != "into"
        || args[5] != "from"
        || args[7] != "to"
    {
        return Err(format!("Invalid syntax \"{}\"", args.join(" ")));
    }
    let count = args[4]
        .parse::<usize>()
        .ok()
        .filter(|n| *n >= 2 && *n <= 128)
        .ok_or("scale: native rung count must be from 2 to 128")?;
    let mut reduce = false;
    let mut taker = false;
    let mut target = symbol.to_string();
    let mut has_symbol = false;
    for token in args.iter().skip(9) {
        match token.as_str() {
            "reduce" if !reduce && !has_symbol => reduce = true,
            "taker" if !taker && !has_symbol => taker = true,
            "cubic" | "icubic" | "exponential" | "reduce" | "taker" => {
                return Err(format!(
                    "modifier '{token}' is not implemented or specified more than once; supported: reduce, taker"
                ));
            }
            _ if !has_symbol => {
                let resolved=resolve(store,token).filter(|s|s.eq_ignore_ascii_case(token)).ok_or_else(||format!("Unknown symbol \"{token}\". Use a valid market symbol, or omit it to trade the focused symbol ({symbol})."))?;
                target = resolved;
                has_symbol = true;
            }
            _ => {
                return Err(format!(
                    "modifier '{token}' is not implemented; supported: reduce, taker"
                ));
            }
        }
    }
    health(store, reduce)?;
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == target)
        .ok_or_else(|| format!("Market not found for {target}"))?;
    let last = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == target)
        .map(|t| n(t, "last"))
        .filter(|n| *n != 0.)
        .ok_or_else(|| format!("Current price is not available for ticker {target}, try again."))?;
    let lot = market["precision"]["amount"]
        .as_f64()
        .filter(|n| *n != 0.)
        .unwrap_or(1e-8);
    let side = if args[1] == "buy" { "buy" } else { "sell" };
    let total = Placement::sized(&target, side, &args[2], reduce).size(store, market, last, lot)?;
    let a = price(&args[6], last)?;
    let b = price(&args[8], last)?;
    let low = a.min(b);
    let increment = (a.max(b) - low) / (count - 1) as f64;
    let mut orders = Vec::with_capacity(count);
    for i in 0..count {
        let mut order = json!({"symbol":target,"side":side,"type":"limit","amount":total/count as f64,"price":low+i as f64*increment,"reduceOnly":reduce});
        if !taker {
            order["timeInForce"] = json!("PostOnly");
        }
        orders.push(prepare_order(order, store)?);
    }
    Ok(Evaluation::Batch {
        orders,
        success: "Created orders {ids}".into(),
    })
}
fn bump(symbol: &str, args: &[String], store: &Value, raw: &str) -> Result<Evaluation, String> {
    let mut index = 1;
    let mut target = symbol.to_string();
    let mut explicit = false;
    if args.len() > 2
        && let Some(s) = resolve(store, &args[1])
    {
        target = s;
        index += 1;
        explicit = true;
    }
    let mut side = None;
    if let Some(token) = args.get(index) {
        match token.as_str() {
            "buy" | "buys" => {
                side = Some("buy");
                index += 1;
            }
            "sell" | "sells" => {
                side = Some("sell");
                index += 1;
            }
            _ => {}
        }
    }
    let mut selector = None;
    if let Some(token) = args.get(index)
        && matches!(token.as_str(), "top" | "bottom")
    {
        let count = args
            .get(index + 1)
            .and_then(|s| s.parse::<usize>().ok())
            .filter(|n| *n > 0)
            .ok_or("Invalid count")?;
        selector = Some((token.as_str(), count));
        index += 2;
    }
    let text = args.get(index).ok_or("Missing amount argument")?;
    let number_text = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    let mut amount = number(number_text)
        .filter(|n| *n != 0.)
        .ok_or("Invalid amount")?;
    if text.starts_with('-') {
        amount = -amount;
    }
    index += 1;
    if let Some(token) = args.get(index) {
        if explicit {
            return Err(format!("Unexpected token {token}"));
        }
        target = resolve(store, token).ok_or_else(|| format!("Unknown symbol \"{token}\""))?;
        index += 1;
    }
    if index < args.len() {
        return Err("Unexpected bump arguments".into());
    }
    if side.is_none() {
        if raw.split_whitespace().any(|t| matches!(t, "--buys" | "-b")) {
            side = Some("buy");
        } else if raw
            .split_whitespace()
            .any(|t| matches!(t, "--sells" | "-s"))
        {
            side = Some("sell");
        }
    }
    let mut orders: Vec<_> = rows(store, "orders")
        .iter()
        .filter(|o| {
            o["symbol"] == target
                && o["status"] == "open"
                && o["type"] == "limit"
                && side.is_none_or(|side| o["side"] == side)
        })
        .cloned()
        .collect();
    if let Some((kind, count)) = selector {
        orders.sort_by(|a, b| n(a, "price").total_cmp(&n(b, "price")));
        if kind == "top" {
            orders.reverse();
        }
        orders.truncate(count);
    }
    if orders.is_empty() {
        return Ok(Evaluation::Text {
            message: format!("No matching limit orders to bump on {target}"),
            result: json!({"handled":true,"success":true}),
        });
    }
    let actions = orders
        .into_iter()
        .map(|order| Action::Amend {
            price: n(&order, "price") + amount,
            order,
        })
        .collect();
    Ok(Evaluation::Sequence {
        actions,
        success: format!(
            "Bumped {{ok}} order(s) {} by {}",
            if amount > 0. { "up" } else { "down" },
            amount.abs()
        ),
        attempt_all: true,
        failure_summary: Some("Bumped {ok} order(s), {failed} failed".into()),
    })
}
