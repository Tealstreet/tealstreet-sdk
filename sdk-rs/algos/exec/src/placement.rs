//! CLI buy/sell grammar and the shared TS order preparation policy.
use crate::{resolve, rows};
use serde_json::{Value, json};
use std::collections::VecDeque;

#[derive(Clone)]
pub(crate) struct Placement {
    symbol: String,
    side: &'static str,
    size: String,
    price: Option<String>,
    best: bool,
    stop: Option<String>,
    reduce: bool,
    post_only: bool,
}
pub(crate) fn number(text: &str) -> Option<f64> {
    let mut dots = 0;
    if text.is_empty()
        || !text.chars().all(|c| {
            c.is_ascii_digit()
                || (c == '.' && {
                    dots += 1;
                    dots == 1
                })
        })
    {
        return None;
    }
    text.parse::<f64>().ok().filter(|n| n.is_finite())
}
fn numeric_token(text: &str) -> bool {
    let text = text.trim_start_matches(['+', '-']);
    number(text.trim_matches('$').split('%').next().unwrap_or("")).is_some()
}
pub(crate) fn parse(
    symbol: &str,
    args: &[String],
    store: &Value,
) -> Result<VecDeque<Placement>, String> {
    let side = if matches!(args[0].as_str(), "buy" | "b") {
        "buy"
    } else {
        "sell"
    };
    let keywords = ["reduce", "po", "at", "best", "stop"];
    for (i, tok) in args.iter().enumerate().skip(1) {
        if keywords.contains(&tok.to_lowercase().as_str())
            || i > 1 && matches!(args[i - 1].as_str(), "at" | "stop")
            || numeric_token(tok)
        {
            continue;
        }
        for part in tok.split(',').filter(|s| !s.is_empty()) {
            if resolve(store, part).is_none() {
                return Err(format!(
                    "Unknown symbol \"{part}\". Use a valid market symbol, or omit it to trade the focused symbol ({symbol})."
                ));
            }
        }
    }
    let symbols = |s: &str| {
        s.split(',')
            .filter_map(|part| resolve(store, part))
            .collect::<Vec<_>>()
    };
    let leading = args.get(1).map(|s| symbols(s)).unwrap_or_default();
    let size_index = if leading.is_empty() { 1 } else { 2 };
    let size = args.get(size_index).ok_or("Size argument missing")?.clone();
    let mut targets = leading;
    if targets.is_empty() {
        for (i, tok) in args.iter().enumerate().rev().filter(|(i, _)| *i >= 2) {
            if keywords.contains(&tok.as_str()) || matches!(args[i - 1].as_str(), "at" | "stop") {
                continue;
            }
            targets = symbols(tok);
            if !targets.is_empty() {
                break;
            }
        }
    }
    if targets.is_empty() {
        targets.push(symbol.into());
    }
    let operand = |key: &str| -> Result<Option<String>, String> {
        args.iter()
            .position(|a| a.eq_ignore_ascii_case(key))
            .map(|i| {
                args.get(i + 1)
                    .cloned()
                    .ok_or_else(|| format!("'{key}' must be followed by a price"))
            })
            .transpose()
    };
    let price = operand("at")?;
    let stop = operand("stop")?;
    let best = args.iter().any(|a| a.eq_ignore_ascii_case("best"));
    if price.is_none() && !best && stop.is_none() {
        for tok in args.iter().skip(size_index + 1) {
            if number(tok).is_some() {
                return Err("Invalid syntax: use \"at\" keyword for limit orders".into());
            }
        }
    }
    Ok(targets
        .into_iter()
        .map(|symbol| Placement {
            symbol,
            side,
            size: size.clone(),
            price: price.clone(),
            best,
            stop: stop.clone(),
            reduce: args.iter().any(|a| a == "reduce"),
            post_only: args.iter().any(|a| a == "po"),
        })
        .collect())
}
pub(crate) fn n(value: &Value, key: &str) -> f64 {
    value[key].as_f64().unwrap_or(0.)
}
fn positive(value: f64, default: f64) -> f64 {
    if value > 0. && value.is_finite() {
        value
    } else {
        default
    }
}
fn js_round(value: f64) -> f64 {
    (value + 0.5).floor()
}
fn clean(value: f64) -> f64 {
    format!("{value:.10}").parse().unwrap_or(0.)
}
pub(crate) fn nearest(value: f64, step: f64) -> f64 {
    clean(js_round(value / step) * step)
}
pub(crate) fn adjust(value: f64, step: f64) -> f64 {
    let rounded = js_round(value * (1. / step)) / (1. / step);
    // TS adjust rounds once more to the number of decimals in the lot step.
    let mut decimals = 0;
    let mut scaled = step;
    while decimals < 20 && (scaled.abs() < 1. || (scaled - scaled.round()).abs() > 1e-8) {
        scaled *= 10.;
        decimals += 1;
    }
    let factor = 10f64.powi(decimals);
    js_round(rounded * factor) / factor
}
pub(crate) fn native_position(position: &Value, market: &Value) -> f64 {
    let contract = positive(n(market, "contractSize"), 1.);
    let source = if market["isInverse"] == true {
        n(position, "notional")
    } else {
        n(position, "contracts")
    };
    n(&position["extra"], "contractsNative")
        .abs()
        .max(source.abs() / contract)
}
pub(crate) fn price(text: &str, last: f64) -> Result<f64, String> {
    let text = text.strip_prefix('$').unwrap_or(text);
    let unsigned = text.trim_start_matches(['+', '-']);
    let value = number(unsigned.trim_end_matches('%'))
        .ok_or_else(|| format!("Price {text} does not look like a price"))?;
    let value = if text.starts_with('-') { -value } else { value };
    Ok(if text.ends_with('%') {
        last + last * value / 100.
    } else if text.starts_with(['+', '-']) {
        last + value
    } else {
        value
    })
}
impl Placement {
    pub(crate) fn sized(symbol: &str, side: &'static str, size: &str, reduce: bool) -> Self {
        Self {
            symbol: symbol.into(),
            side,
            size: size.into(),
            price: None,
            best: false,
            stop: None,
            reduce,
            post_only: false,
        }
    }
    pub(crate) fn prepare(&self, store: &Value) -> Result<Value, String> {
        let symbol = &self.symbol;
        let market = rows(store, "markets")
            .iter()
            .find(|m| m["symbol"] == *symbol)
            .ok_or_else(|| format!("Market not found for {symbol}"))?;
        let ticker = rows(store, "tickers")
            .iter()
            .find(|t| t["symbol"] == *symbol)
            .unwrap_or(&Value::Null);
        let last = n(ticker, "last");
        let limit_price = if self.best {
            let best = n(ticker, if self.side == "buy" { "bid" } else { "ask" });
            let p = if best != 0. { best } else { last };
            if p == 0. {
                return Err(format!("No bid/ask available for {symbol}"));
            }
            Some(p)
        } else if let Some(text) = &self.price {
            if last == 0. {
                return Err(format!(
                    "Current price is not available for ticker {symbol}, try again."
                ));
            }
            Some(price(text, last)?)
        } else {
            None
        };
        let stop = if let Some(text) = &self.stop {
            if last == 0. {
                return Err(format!("Current price not available for {symbol}"));
            }
            Some(price(text, last)?)
        } else {
            None
        };
        if !self.reduce
            && store["socketHealth"]["privateInitialized"] == true
            && store["socketHealth"]["privateHealthy"] == false
        {
            return Err("Account connection is degraded (private data stream is stale) — refusing to open or increase exposure. Wait for reconnect (run `health`); close / cancel / reduce-only orders stay allowed.".into());
        }
        let lot = positive(n(&market["precision"], "amount"), 1e-8);
        let amount = self.size(store, market, last, lot)?;
        if amount <= 0. || !amount.is_finite() {
            return Err(format!(
                "Size \"{}\" rounds to 0 {symbol} at this market's precision — order not placed.",
                self.size
            ));
        }
        let mut order = json!({"symbol":symbol,"side":self.side,"type":if limit_price.is_some(){"limit"}else{"market"},"amount":amount,"reduceOnly":self.reduce,"timeInForce":if limit_price.is_some()&&self.post_only{"PostOnly"}else{"GoodTillCancel"}});
        if let Some(price) = limit_price {
            order["price"] = json!(price);
        }
        if let Some(stop) = stop {
            order["stopLoss"] = json!(stop);
        }
        prepare_order(order, store)
    }

    pub(crate) fn size(
        &self,
        store: &Value,
        market: &Value,
        last: f64,
        lot: f64,
    ) -> Result<f64, String> {
        let size = self.size.trim();
        let inverse = market["isInverse"] == true;
        let contract = positive(n(market, "contractSize"), 1.);
        let from_notional = |notional: f64| -> Result<f64, String> {
            if inverse {
                Ok(adjust(notional / contract, lot))
            } else if last != 0. {
                Ok(adjust(notional / last / contract, lot))
            } else {
                Err("Price data is not available yet for a symbol".into())
            }
        };
        if let Some(usd) = size
            .strip_prefix('$')
            .or_else(|| size.strip_suffix('$'))
            .and_then(number)
        {
            return from_notional(usd);
        }
        let (scope, pct) = if let Some(p) = size.strip_suffix("%bal") {
            ("balance", p)
        } else if let Some(p) = size.strip_suffix("%pos") {
            ("position", p)
        } else if let Some(p) = size.strip_suffix('%') {
            (if self.reduce { "position" } else { "balance" }, p)
        } else {
            return Err(format!(
                "Invalid size \"{size}\". Use $1000, 1000$, or NN% (or NN%bal / NN%pos)."
            ));
        };
        let pct = number(pct.trim_start_matches('+'))
            .filter(|p| *p > 0.)
            .ok_or_else(|| format!("Invalid percentage: {size}"))?;
        if scope == "position" {
            let side = if self.side == "buy" { "short" } else { "long" };
            let p = rows(store, "positions")
                .iter()
                .find(|p| p["symbol"] == self.symbol && (!self.reduce || p["side"] == side))
                .ok_or_else(|| {
                    format!(
                        "No {}position found for symbol {}",
                        if self.reduce {
                            format!("{side} ")
                        } else {
                            String::new()
                        },
                        self.symbol
                    )
                })?;
            Ok(adjust(native_position(p, market) * pct / 100., lot))
        } else {
            let balance = store
                .get("balance")
                .filter(|b| !b.is_null())
                .ok_or("Balance not available yet, try again")?;
            let lev = rows(store, "positions")
                .iter()
                .find(|p| p["symbol"] == self.symbol && n(p, "contracts") != 0.)
                .and_then(|p| p["leverage"].as_f64())
                .unwrap_or(1.);
            from_notional(n(balance, "free") * lev * pct / 100.)
        }
    }
}
fn clamp(order: &mut Value, store: &Value, market: &Value, ticker: &Value) {
    if order["reduceOnly"] != true
        || !crate::capabilities::clamp(store["venue"].as_str().unwrap_or(""))
    {
        return;
    }
    let price = positive(
        n(order, "price"),
        positive(n(ticker, "mark"), n(ticker, "last")),
    );
    if price == 0. {
        return;
    }
    let side = if order["side"] == "buy" {
        "short"
    } else {
        "long"
    };
    let Some(position) = rows(store, "positions")
        .iter()
        .find(|p| p["symbol"] == order["symbol"] && p["side"] == side && n(p, "contracts") > 0.)
    else {
        return;
    };
    let contract = positive(n(market, "contractSize"), 1.);
    let conversion = if market["isInverse"] != true && contract < 1. {
        contract
    } else {
        1.
    };
    let mut reduced = (n(order, "amount") * conversion).min(n(position, "contracts"));
    for o in rows(store, "orders").iter().filter(|o| {
        o["symbol"] == order["symbol"]
            && o["side"] == order["side"]
            && o["type"] == "limit"
            && matches!(o["status"].as_str(), Some("open" | "creating"))
            && (position["positionMode"] != "hedged" || o["reduceOnly"] == true)
            && if order["side"] == "buy" {
                n(o, "price") >= price
            } else {
                n(o, "price") <= price
            }
    }) {
        reduced -= n(o, "amount");
    }
    if reduced > 0. {
        order["amount"] =
            json!(nearest(reduced, positive(n(&market["precision"], "amount"), 1e-8)) / conversion);
    }
}
fn tradfi_preflight(store: &Value, market: &Value) -> Result<(), String> {
    if !matches!(
        store["venue"].as_str(),
        Some("bybit" | "bybitdemo" | "bybitinverse" | "bybitspot")
    ) {
        return Ok(());
    }
    if store["instrumentClassificationsReady"] == false {
        return Err("Instrument classification is not loaded yet — order not placed".into());
    }
    let label = market["info"]["symbolType"]
        .as_str()
        .or_else(|| market["instrumentClassification"]["symbolType"].as_str())
        .unwrap_or("")
        .trim();
    let asset = match label {
        "stock" => "tokenized equity",
        "ETF" => "tokenized ETF",
        "commodity" => "tokenized commodity",
        "forex" => "tokenized FX pair",
        _ => return Ok(()),
    };
    let underlying = market["info"]["underlyingTicker"]
        .as_str()
        .or_else(|| market["instrumentClassification"]["underlyingTicker"].as_str())
        .filter(|s| !s.is_empty())
        .map(|s| format!(" ({s})"))
        .unwrap_or_default();
    Err(format!(
        "{} on {} is a {asset}{underlying}. Opening a position may require an agreement you have not signed with the venue — place the order through the venue's own UI first to accept it.",
        market["symbol"].as_str().unwrap_or(""),
        store["venue"].as_str().unwrap_or("")
    ))
}

pub(crate) fn prepare_order(mut order: Value, store: &Value) -> Result<Value, String> {
    let symbol = order["symbol"].as_str().unwrap_or("").to_string();
    let market = rows(store, "markets")
        .iter()
        .find(|m| m["symbol"] == symbol)
        .ok_or_else(|| format!("Market not found for {symbol}"))?;
    let ticker = rows(store, "tickers")
        .iter()
        .find(|t| t["symbol"] == symbol)
        .unwrap_or(&Value::Null);
    tradfi_preflight(store, market)?;
    let lot = positive(n(&market["precision"], "amount"), 1e-8);
    let tick = positive(n(&market["precision"], "price"), 0.01);
    let amount = n(&order, "amount");
    let infinity = amount == 0.
        && matches!(
            order["type"].as_str(),
            Some("stop_market" | "take_profit_market" | "trailing_stop_market")
        );
    if infinity && !crate::capabilities::infinity(store["venue"].as_str().unwrap_or("")) {
        return Err(format!(
            "'{}' does not support Full (Infinity) TP/SL orders",
            store["venue"].as_str().unwrap_or("this venue")
        ));
    }
    let reduce_market = order["reduceOnly"] == true && order["type"] == "market";
    let down = market["isSpot"] == true
        && matches!(
            store["venue"].as_str(),
            Some("hyperliquid" | "hyperliquidspot")
        )
        && !reduce_market;
    let quotient = amount / lot;
    let closest = js_round(quotient);
    let mut rounded = if down {
        clean(
            (if (quotient - closest).abs() <= quotient.abs().max(1.) * 1e-12 {
                closest
            } else {
                quotient.floor()
            }) * lot,
        )
    } else {
        nearest(amount, lot)
    };
    if rounded <= 0. && reduce_market {
        rounded = amount;
    }
    if !infinity && (!rounded.is_finite() || rounded <= 0.) {
        return Err(format!(
            "order size {amount} rounds to {rounded} for '{symbol}' — below the venue lot step"
        ));
    }
    order["amount"] = json!(rounded);
    for field in ["price", "stopLoss", "takeProfit", "triggerPrice"] {
        if let Some(value) = order[field].as_f64() {
            let rounded = nearest(value, tick);
            if !rounded.is_finite() || rounded <= 0. {
                return Err(format!("invalid {field} for '{symbol}': {value}"));
            }
            if field == "price"
                && market["limits"]["price"]["min"]
                    .as_f64()
                    .is_some_and(|min| rounded < min)
            {
                return Err(format!(
                    "price {rounded} is below the minimum {} for '{symbol}'",
                    market["limits"]["price"]["min"]
                ));
            }
            order[field] = json!(rounded);
        }
    }
    clamp(&mut order, store, market, ticker);
    Ok(order)
}
