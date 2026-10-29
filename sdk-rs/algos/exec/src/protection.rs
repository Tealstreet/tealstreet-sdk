//! Close, native protective orders and price-only amendments.
use crate::placement::{Placement, n, native_position, nearest, number, prepare_order, price};
use crate::{Action, Evaluation, done, failed, resolve, rows};
use serde_json::{Value, json};
fn target(symbol: &str, args: &[String], store: &Value, skip: &[&str]) -> String {
    args.iter()
        .skip(1)
        .rev()
        .filter(|s| !skip.contains(&s.as_str()))
        .find_map(|s| resolve(store, s))
        .unwrap_or_else(|| symbol.into())
}
fn sequence(actions: Vec<Action>, success: impl Into<String>, attempt_all: bool) -> Evaluation {
    Evaluation::Sequence {
        actions: actions.into(),
        success: success.into(),
        attempt_all,
    }
}
fn ticker<'a>(store: &'a Value, symbol: &str) -> &'a Value {
    rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .unwrap_or(&Value::Null)
}
fn positions<'a>(store: &'a Value, symbol: &str) -> Vec<&'a Value> {
    rows(store, "positions")
        .iter()
        .filter(|p| p["symbol"] == symbol && n(p, "contracts") != 0.)
        .collect()
}
fn protective(symbol: &str, side: &str, mark: f64, trigger: f64, limit: f64, amount: f64) -> Value {
    let tp = if side == "sell" {
        trigger > mark
    } else {
        trigger <= mark
    };
    json!({"symbol":symbol,"side":side,"type":if tp{"take_profit_market"}else{"stop_market"},"price":limit,"triggerPrice":trigger,"amount":amount,"reduceOnly":true,"timeInForce":"GoodTillCancel"})
}
pub(crate) fn evaluate(symbol: &str, args: &[String], store: &Value, raw: &str) -> Evaluation {
    if !matches!(args[0].as_str(), "move" | "mv" | "bump") {
        for (i, token) in args.iter().enumerate().skip(1) {
            if [
                "long", "short", "buy", "sell", "at", "trigger", "all", "longs", "shorts", "allpos",
            ]
            .contains(&token.as_str())
                || token.contains('$')
                || token.contains('%')
                || price(token, 1.).is_ok()
                || i > 1 && matches!(args[i - 1].as_str(), "at" | "trigger")
            {
                continue;
            }
            if token.split(',').any(|part| resolve(store, part).is_none()) {
                return failed(format!("Unknown symbol \"{token}\""));
            }
        }
    }
    let result = match args[0].as_str() {
        "close" | "c" => close(symbol, args, store),
        "stop" => stop(symbol, args, store, raw),
        "be" | "breakeven" => breakeven(symbol, args, store),
        "trail" => trail(symbol, args, store, raw),
        "move" | "mv" => move_order(args, store),
        "bump" => Err("Not implemented".into()),
        _ => unreachable!(),
    };
    result.unwrap_or_else(failed)
}
fn close(symbol: &str, args: &[String], store: &Value) -> Result<Evaluation, String> {
    let head = args.get(1).map(String::as_str).unwrap_or("");
    let global = matches!(head, "longs" | "shorts" | "allpos" | "all");
    let long = args.iter().any(|a| a == "long");
    let short = args.iter().any(|a| a == "short");
    if long && short {
        return Err("Cannot have both long and short closing requests".into());
    }
    let size = args
        .iter()
        .skip(1)
        .find(|s| s.starts_with('$') || s.ends_with('$') || s.contains('%'))
        .cloned();
    let mut symbols: Vec<String> = args
        .iter()
        .skip(1)
        .take(3)
        .filter(|a| Some(*a) != size.as_ref() && !matches!(a.as_str(), "long" | "short"))
        .flat_map(|a| a.split(',').filter_map(|s| resolve(store, s)))
        .collect();
    if symbols.is_empty() {
        symbols.push(symbol.into());
    }
    let mut actions = Vec::new();
    if global {
        for position in rows(store, "positions").iter().filter(|p| {
            n(p, "contracts") != 0.
                && (head != "longs" || p["side"] == "long")
                && (head != "shorts" || p["side"] == "short")
        }) {
            actions.push(Action::Close {
                position: position.clone(),
                size: size.clone(),
            });
        }
    } else {
        for symbol in symbols {
            let selected: Vec<_> = positions(store, &symbol)
                .into_iter()
                .filter(|p| {
                    (!long && !short)
                        || (long && p["side"] == "long")
                        || (short && p["side"] == "short")
                })
                .collect();
            if size.is_some() && selected.len() > 1 {
                return Err(format!(
                    "Multiple positions on {symbol} — use 'close long <size>' or 'close short <size>' to disambiguate"
                ));
            }
            for position in selected {
                actions.push(Action::Close {
                    position: position.clone(),
                    size: size.clone(),
                });
            }
        }
    }
    let success = if global {
        let label = if head == "longs" {
            "long"
        } else if head == "shorts" {
            "short"
        } else {
            "open"
        };
        if size.is_some() {
            format!("Partial close placed on {{ok}} {label} position(s)")
        } else {
            format!("Closing {{ok}} {label} position(s)")
        }
    } else if size.is_some() {
        "Partial close placed".into()
    } else {
        "Positions closed".into()
    };
    Ok(sequence(actions, success, global))
}
pub(crate) fn prepare_close(
    position: &Value,
    size: Option<&str>,
    store: &Value,
) -> Result<Value, String> {
    let symbol = position["symbol"]
        .as_str()
        .ok_or("Missing position symbol")?;
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
        .ok_or_else(|| format!("Market not found for {symbol}"))?;
    let full = native_position(position, market);
    let side = if position["side"] == "long" {
        "sell"
    } else {
        "buy"
    };
    let amount = if let Some(size) = size {
        Placement::sized(symbol, side, size, true)
            .size(
                store,
                market,
                n(ticker(store, symbol), "last"),
                market["precision"]["amount"]
                    .as_f64()
                    .filter(|n| *n != 0.)
                    .unwrap_or(1e-8),
            )?
            .abs()
            .min(full)
    } else {
        full
    };
    prepare_order(
        json!({"symbol":symbol,"side":side,"type":"market","amount":amount,"reduceOnly":true}),
        store,
    )
}
fn stop(symbol: &str, args: &[String], store: &Value, raw: &str) -> Result<Evaluation, String> {
    let target = target(symbol, args, store, &["buy", "sell", "at", "trigger"]);
    let size_index = if args
        .get(1)
        .is_some_and(|s| s.starts_with('$') || s.ends_with('$'))
    {
        Some(1)
    } else if args
        .get(1)
        .is_some_and(|s| matches!(s.as_str(), "buy" | "sell"))
        && args
            .get(2)
            .is_some_and(|s| s.starts_with('$') || s.ends_with('$'))
    {
        Some(2)
    } else {
        None
    };
    let index = args
        .iter()
        .position(|s| s == "at")
        .map(|i| i + 1)
        .or_else(|| {
            args.iter()
                .enumerate()
                .skip(1)
                .find(|(i, s)| Some(*i) != size_index && price(s, 1.).is_ok())
                .map(|(i, _)| i)
        });
    let text=index.and_then(|i|args.get(i)).ok_or_else(||format!("stop: could not find a trigger price in \"{raw}\". Expected e.g. `stop 18500`, `stop at -1%`, `stop sell $500 at 1%`."))?;
    let t = ticker(store, &target);
    let last = n(t, "last");
    if last == 0. {
        return Err(format!(
            "Current price is not available for ticker {target}, try again."
        ));
    }
    let limit = price(text, last)?;
    let trigger = if let Some(i) = args.iter().position(|s| s == "trigger") {
        let text = args
            .get(i + 1)
            .ok_or("trigger: missing absolute price argument")?;
        number(text).filter(|p| *p > 0.).ok_or_else(|| {
            format!("Invalid price: trigger '{text}' must be an absolute positive price")
        })?
    } else {
        limit
    };
    let side = if args.iter().any(|s| s == "buy") {
        "buy"
    } else if args.iter().any(|s| s == "sell") {
        "sell"
    } else {
        let selected = positions(store, &target);
        if selected.len() > 1 {
            return Err(
                "More than one position are opened, specify <buy|sell> in a command.".into(),
            );
        }
        if selected.is_empty() {
            return Err(format!("No position found for {target}"));
        }
        if selected[0]["side"] == "long" {
            "sell"
        } else {
            "buy"
        }
    };
    let amount = if let Some(i) = size_index {
        let market = rows(store, "markets")
            .iter()
            .find(|m| m["symbol"] == target)
            .unwrap_or(&Value::Null);
        Placement::sized(&target, side, &args[i], false).size(
            store,
            market,
            last,
            market["precision"]["amount"]
                .as_f64()
                .filter(|n| *n != 0.)
                .unwrap_or(1e-8),
        )?
    } else {
        0.
    };
    let mark = t.get("mark").and_then(Value::as_f64).unwrap_or(last);
    Ok(sequence(
        vec![Action::Order(protective(
            &target, side, mark, trigger, limit, amount,
        ))],
        if args.iter().any(|s| s == "trigger") {
            "Placing a limit stop order"
        } else {
            "Placing a stop order"
        },
        false,
    ))
}
fn breakeven(symbol: &str, args: &[String], store: &Value) -> Result<Evaluation, String> {
    let target = target(symbol, args, store, &[]);
    let selected = positions(store, &target);
    if selected.is_empty() {
        return Err(format!("No position found for {target}"));
    }
    if selected.len() > 1 {
        return Err(format!(
            "More than one position is open on {target}; close one or use `stop` directly."
        ));
    }
    let p = selected[0];
    let t = ticker(store, &target);
    let mark = t
        .get("mark")
        .and_then(Value::as_f64)
        .unwrap_or(n(t, "last"));
    if mark == 0. {
        return Err(format!(
            "Current price is not available for ticker {target}, try again."
        ));
    }
    let base = n(p, "entryPrice");
    let offset = args.iter().skip(1).find(|s| price(s, base).is_ok());
    let value = offset.map(|s| price(s, base)).transpose()?.unwrap_or(base);
    if value <= 0. || !value.is_finite() {
        return Err(format!(
            "Break-even price for {target} is not a usable price ({value})."
        ));
    }
    let side = if p["side"] == "long" { "sell" } else { "buy" };
    let stops: Vec<_> = rows(store, "orders")
        .iter()
        .filter(|o| o["symbol"] == target && o["type"] == "stop_market" && o["side"] == side)
        .cloned()
        .collect();
    let mut actions = Vec::new();
    if !stops.is_empty() {
        actions.push(Action::Call("cancelOrders", json!([stops])));
    }
    let order = protective(&target, side, mark, value, value, 0.);
    let tp = order["type"] == "take_profit_market";
    actions.push(if actions.is_empty() {
        Action::Order(order)
    } else {
        Action::Replacement(order)
    });
    let suffix = offset
        .map(|s| format!(" (break-even {base}, offset {s})"))
        .unwrap_or_default();
    let success = if tp {
        format!(
            "Stop moved to {value}{suffix}\nBreak-even for {target} is on the far side of the market, so that is a reduce-only take-profit at {value}, not a stop — the position has no stop loss now."
        )
    } else {
        format!("Stop moved to {value}{suffix}")
    };
    Ok(sequence(actions, success, false))
}
fn trail(symbol: &str, args: &[String], store: &Value, raw: &str) -> Result<Evaluation, String> {
    let venue = store["venue"].as_str().unwrap_or("");
    if !crate::capabilities::trailing(venue) {
        return Err(format!("{venue} does not support trailing stops."));
    }
    let target = target(symbol, args, store, &[]);
    let distance = args
        .iter()
        .skip(1)
        .find(|s| price(s, 1.).is_ok())
        .ok_or_else(|| {
            format!("trail: no distance in \"{raw}\". Expected e.g. `trail 20%` or `trail 500`.")
        })?;
    let selected = positions(store, &target);
    if selected.is_empty() {
        return Err(format!("No position found for {target}"));
    }
    if selected.len() > 1 {
        return Err(format!(
            "More than one position is open on {target}; close one first."
        ));
    }
    let last = n(ticker(store, &target), "last");
    if last == 0. {
        return Err(format!(
            "Current price is not available for ticker {target}, try again."
        ));
    }
    let d = number(
        distance
            .trim_end_matches('%')
            .trim_start_matches(['+', '-']),
    )
    .unwrap_or(0.)
        * if distance.ends_with('%') {
            last / 100.
        } else {
            1.
        };
    if d <= 0. || !d.is_finite() {
        return Err(format!("trail: \"{distance}\" is not a usable distance."));
    }
    let long = selected[0]["side"] == "long";
    let price = if long { last - d } else { last + d };
    if price <= 0. {
        return Err(format!(
            "trail: a distance of {distance} puts the stop at {price}, below zero."
        ));
    }
    Ok(sequence(
        vec![Action::Order(
            json!({"symbol":target,"side":if long{"sell"}else{"buy"},"type":"trailing_stop_market","timeInForce":"GoodTillCancel","reduceOnly":true,"price":price,"amount":0}),
        )],
        format!("Trailing stop {distance} behind {last} (currently {price})"),
        false,
    ))
}

fn move_order(args: &[String], store: &Value) -> Result<Evaluation, String> {
    let (Some(id), Some(text)) = (args.get(1), args.get(2)) else {
        return Err("Missing arguments".into());
    };
    let selected: Vec<_> = rows(store, "orders")
        .iter()
        .filter(|o| {
            o["id"].as_str().is_some_and(|s| s.starts_with(id))
                && o["status"] == "open"
                && o["type"] == "limit"
        })
        .collect();
    if selected.is_empty() {
        return Err("Order not found".into());
    }
    if selected.len() > 1 {
        return Err("Ambiguous order ID".into());
    }
    let order = selected[0];
    let numeric = text
        .strip_prefix('+')
        .or_else(|| text.strip_prefix('-'))
        .unwrap_or(text);
    let raw = number(numeric)
        .map(|n| if text.starts_with('-') { -n } else { n })
        .ok_or("Invalid price")?;
    let price = if text.starts_with(['+', '-']) {
        n(order, "price") + raw
    } else {
        raw
    };
    if price <= 0. {
        return Err("Invalid price".into());
    }
    if price == n(order, "price") {
        return Ok(done());
    }
    let symbol = order["symbol"].as_str().unwrap_or("");
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
        .ok_or_else(|| format!("Market not found for {symbol}"))?;
    let step = market["precision"]["price"]
        .as_f64()
        .filter(|p| *p != 0.)
        .unwrap_or(0.01);
    let rounded = nearest(price, step);
    if rounded <= 0. || !rounded.is_finite() {
        return Err(format!(
            "price {price} rounds to {rounded} for '{symbol}' — below the venue tick"
        ));
    }
    if market["limits"]["price"]["min"]
        .as_f64()
        .is_some_and(|min| rounded < min)
    {
        return Err(format!(
            "price {rounded} is below the minimum {} for '{symbol}'",
            market["limits"]["price"]["min"]
        ));
    }
    Ok(sequence(
        vec![Action::Call(
            "updateOrder",
            json!([{"order":order,"update":{"price":rounded}}]),
        )],
        format!("Order moved to {price}"),
        false,
    ))
}
