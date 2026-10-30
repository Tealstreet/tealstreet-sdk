//! `callMethod {method, args}` to the engine command JSON that `RustEngineExchange`
//! (safe-cex `rust.exchange.ts`) sends to `WasmEngine.command`. Every produced
//! command still passes the engine's typed decoder; an unknown method never
//! gets a guessed command.
use safe_cex_engine::Capabilities;
use safe_cex_engine::capabilities::{CancelAll, FillHistoryTime, LeverageQuery, MaxLeverage};
use serde_json::{Map, Value, json};

/// Client `args` larger than this are refused before translation.
pub const MAX_ARGS_BYTES: usize = 64 * 1024;

/// What a method becomes: one engine command, or a direct reply.
#[derive(Debug, PartialEq)]
pub enum Plan {
    /// `returns_unit`: the TS method returns `undefined`, not the engine result.
    Command { command: Value, returns_unit: bool },
    /// Answered without the engine (`cancelAllOrders` with nothing to cancel).
    Reply(Value),
}

#[derive(Debug, PartialEq)]
pub struct Refused {
    pub name: &'static str,
    pub message: String,
}

fn refused(name: &'static str, message: impl Into<String>) -> Refused {
    Refused {
        name,
        message: message.into(),
    }
}

fn arg(args: &[Value], index: usize) -> &Value {
    args.get(index).unwrap_or(&Value::Null)
}

fn text(value: &Value, what: &str) -> Result<String, Refused> {
    value
        .as_str()
        .map(str::to_owned)
        .ok_or_else(|| refused("TypeError", format!("{what} must be a string")))
}

fn finite(value: &Value, what: &str) -> Result<f64, Refused> {
    value
        .as_f64()
        .filter(|number| number.is_finite())
        .ok_or_else(|| refused("TypeError", format!("{what} must be a finite number")))
}

/// The order fields a client may send; account and credential fields never pass.
fn order(value: &Value) -> Result<Value, Refused> {
    let Value::Object(fields) = value else {
        return Err(refused("TypeError", "order must be an object"));
    };
    const DENIED: [&str; 6] = [
        "accountName",
        "account",
        "key",
        "secret",
        "passphrase",
        "credentials",
    ];
    if let Some(field) = DENIED.iter().find(|field| fields.contains_key(**field)) {
        return Err(refused(
            "TypeError",
            format!("order field {field} is not allowed"),
        ));
    }
    for field in [
        "amount",
        "price",
        "triggerPrice",
        "stopLoss",
        "takeProfit",
        "maxSlippagePct",
    ] {
        if let Some(number) = fields.get(field).filter(|number| !number.is_null()) {
            finite(number, field)?;
        }
    }
    Ok(Value::Object(fields.clone()))
}

fn command(op: &str, fields: Map<String, Value>, returns_unit: bool) -> Plan {
    let mut command = Map::new();
    command.insert("op".into(), op.into());
    command.extend(fields);
    Plan::Command {
        command: Value::Object(command),
        returns_unit,
    }
}

fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

/// An optional history field the engine reads as `u32`; a JS number is accepted
/// only when it is a whole number in range, so it never reaches it as `10.0`.
fn limit(value: Option<&Value>) -> Result<Option<Value>, Refused> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(None),
        Some(value) => value
            .as_f64()
            .filter(|limit| limit.fract() == 0.0 && *limit >= 0.0 && *limit <= f64::from(u32::MAX))
            .map(|limit| Some((limit as u32).into()))
            .ok_or_else(|| refused("TypeError", "limit must be a whole number")),
    }
}

/// An optional history bound or filter, passed through when present.
fn optional(options: &Value, field: &str) -> Option<Value> {
    options.get(field).filter(|value| !value.is_null()).cloned()
}

/// As `fetchHistoricalFills`: seconds become milliseconds where the venue's
/// history clock needs it, and a negative or non-finite bound is dropped.
fn fill_bound(value: Option<Value>, time: FillHistoryTime) -> Option<Value> {
    if time == FillHistoryTime::AsProvided {
        return value;
    }
    let value = value?.as_f64().filter(|value| value.is_finite())?;
    let nonnegative = time == FillHistoryTime::NonnegativeSecondsOrMilliseconds;
    if nonnegative && value < 0.0 {
        return None;
    }
    let milliseconds = if nonnegative {
        value >= 1e12
    } else {
        value > 1e12
    };
    let value = if milliseconds { value } else { value * 1000.0 };
    Some(value.floor().into())
}

/// Translates one hosted `callMethod`. `open_order_symbols` (engine store) answers
/// cancel-all for venues whose engine cancels one symbol at a time; a
/// `public_only` account has no private reads.
pub fn translate(
    method: &str,
    args: &[Value],
    capabilities: &Capabilities,
    open_order_symbols: &[String],
    public_only: bool,
) -> Result<Vec<Plan>, Refused> {
    let size = serde_json::to_string(args).map_or(usize::MAX, |json| json.len());
    if size > MAX_ARGS_BYTES {
        return Err(refused("RangeError", "callMethod arguments are too large"));
    }
    let one = |plan| Ok(vec![plan]);
    match method {
        "placeOrders" => {
            let Value::Array(orders) = arg(args, 0) else {
                return Err(refused("TypeError", "placeOrders takes an array of orders"));
            };
            let orders = orders.iter().map(order).collect::<Result<Vec<_>, _>>()?;
            one(command(
                "placeOrders",
                fields([("orders", Value::Array(orders))]),
                false,
            ))
        }
        "placeOrder" | "placeOrderWithClientOrderId" => {
            let mut order = order(arg(args, 0))?;
            if method == "placeOrderWithClientOrderId" {
                let id = text(arg(args, 1), "clientOrderId")?;
                order["clientOrderId"] = id.into();
            }
            one(command(
                "placeOrders",
                fields([("orders", json!([order]))]),
                false,
            ))
        }
        "updateOrder" => {
            let request = arg(args, 0);
            let existing = &request["order"];
            let update = &request["update"];
            let id = text(&existing["id"], "order.id")?;
            let mut out = fields([("id", id.into())]);
            for field in ["price", "amount"] {
                if let Some(value) = update.get(field) {
                    out.insert(field.into(), finite(value, field)?.into());
                }
            }
            if !out.contains_key("price") && !out.contains_key("amount") {
                return Err(refused(
                    "Error",
                    "Order amendment requires a price or amount",
                ));
            }
            let op = if capabilities.amend_with_symbol {
                out.insert(
                    "symbol".into(),
                    text(&existing["symbol"], "order.symbol")?.into(),
                );
                "amendOrderWithSymbol"
            } else {
                "amendOrder"
            };
            one(command(op, out, false))
        }
        "cancelOrders" => {
            let Value::Array(orders) = arg(args, 0) else {
                return Err(refused(
                    "TypeError",
                    "cancelOrders takes an array of orders",
                ));
            };
            let mut descriptors = Vec::with_capacity(orders.len());
            let mut ids = Vec::with_capacity(orders.len());
            for order in orders {
                let id = text(&order["id"], "order.id")?;
                let symbol = text(&order["symbol"], "order.symbol")?;
                descriptors.push(json!({"id": id, "symbol": symbol, "type": order["type"]}));
                ids.push(Value::String(id));
            }
            if capabilities.cancel_with_symbols {
                one(command(
                    "cancelOrdersWithSymbols",
                    fields([("orders", Value::Array(descriptors))]),
                    true,
                ))
            } else {
                one(command(
                    "cancelOrders",
                    fields([
                        ("ids", Value::Array(ids)),
                        ("orders", Value::Array(descriptors)),
                    ]),
                    true,
                ))
            }
        }
        "cancelSymbolOrders" => {
            let symbol = text(arg(args, 0), "symbol")?;
            one(command(
                "cancelSymbolOrders",
                fields([("symbol", symbol.into())]),
                true,
            ))
        }
        "cancelAllOrders" => match capabilities.cancel_all {
            CancelAll::Native => one(command(
                "cancelSymbolOrders",
                fields([("symbol", "".into())]),
                true,
            )),
            CancelAll::EachOpenSymbol | CancelAll::EachPlacedSymbol => {
                let mut symbols = open_order_symbols.to_vec();
                symbols.sort();
                symbols.dedup();
                if symbols.is_empty() {
                    return one(Plan::Reply(Value::Null));
                }
                Ok(symbols
                    .into_iter()
                    .map(|symbol| {
                        command(
                            "cancelSymbolOrders",
                            fields([("symbol", symbol.into())]),
                            true,
                        )
                    })
                    .collect())
            }
        },
        "findOrderByClientOrderId" if capabilities.client_order_lookup => {
            let symbol = text(arg(args, 0), "symbol")?;
            let id = text(arg(args, 1), "clientOrderId")?;
            one(command(
                "findOrderByClientOrderId",
                fields([("symbol", symbol.into()), ("clientOrderId", id.into())]),
                false,
            ))
        }
        "setLeverage" => {
            let symbol = text(arg(args, 0), "symbol")?;
            let leverage = finite(arg(args, 1), "leverage")?;
            let mut out = fields([("symbol", symbol.into()), ("leverage", leverage.into())]);
            if let Some(side) = args.get(2).filter(|side| !side.is_null()) {
                out.insert("side".into(), text(side, "side")?.into());
            }
            one(command("setLeverage", out, true))
        }
        "setAllLeverage" if capabilities.account_leverage => {
            let leverage = finite(arg(args, 0), "leverage")?;
            one(command(
                "setLeverage",
                fields([("symbol", "".into()), ("leverage", leverage.into())]),
                true,
            ))
        }
        "changeMarginType" => {
            let isolated = arg(args, 0)
                .as_bool()
                .ok_or_else(|| refused("TypeError", "isolated must be a boolean"))?;
            let symbol = match arg(args, 1) {
                Value::Null => {
                    if !capabilities.account_margin_type {
                        return Err(refused("Error", "Symbol is required"));
                    }
                    String::new()
                }
                symbol => text(symbol, "symbol")?,
            };
            one(command(
                "setMarginType",
                fields([("symbol", symbol.into()), ("isolated", isolated.into())]),
                true,
            ))
        }
        "changePositionMode" => {
            let hedged = arg(args, 0)
                .as_bool()
                .ok_or_else(|| refused("TypeError", "hedged must be a boolean"))?;
            let mut out = fields([("hedged", hedged.into())]);
            if let Some(symbol) = args.get(1).filter(|symbol| !symbol.is_null()) {
                out.insert("symbol".into(), text(symbol, "symbol")?.into());
            }
            one(command("setPositionMode", out, true))
        }
        // As `RustEngineExchange.updateSymbolMetadata`: the venue's rails or maximum
        // reach the engine store, and the next frame carries them.
        "updateSymbolMetadata" => {
            let symbol = text(arg(args, 0), "symbol")?;
            if public_only {
                return one(Plan::Reply(Value::Null));
            }
            let op = if capabilities.max_leverage == MaxLeverage::Query {
                "fetchMaxLeverage"
            } else if matches!(
                capabilities.leverage_query,
                LeverageQuery::OnMetadata | LeverageQuery::OnReadAndMetadata
            ) {
                "fetchLeverage"
            } else {
                return one(Plan::Reply(Value::Null));
            };
            one(command(op, fields([("symbol", symbol.into())]), true))
        }
        "fetchHistoricalFills" if capabilities.fill_history => {
            let options = arg(args, 0);
            let (from, to) = (optional(options, "from"), optional(options, "to"));
            if capabilities.finite_history_bounds
                && [&from, &to]
                    .into_iter()
                    .flatten()
                    .any(|bound| !bound.as_f64().is_some_and(f64::is_finite))
            {
                return Err(refused("Error", "history bounds must be finite"));
            }
            let mut out = Map::new();
            for field in ["symbol", "orderId"] {
                if let Some(value) = optional(options, field) {
                    out.insert(field.into(), text(&value, field)?.into());
                }
            }
            let time = capabilities.fill_history_time;
            for (field, bound) in [("from", from), ("to", to)] {
                if let Some(bound) = fill_bound(bound, time) {
                    out.insert(field.into(), finite(&bound, field)?.into());
                }
            }
            if let Some(limit) = limit(options.get("limit"))? {
                out.insert("limit".into(), limit);
            }
            one(command("fetchFills", out, false))
        }
        "fetchHistoricalOrders" if capabilities.order_history => {
            let options = arg(args, 0);
            if capabilities.reject_order_history_filters
                && (optional(options, "orderId").is_some() || optional(options, "status").is_some())
            {
                return Err(refused(
                    "Error",
                    "order history does not support orderId or status filters",
                ));
            }
            let mut out = Map::new();
            if let Some(symbol) = optional(options, "symbol") {
                out.insert("symbol".into(), text(&symbol, "symbol")?.into());
            }
            for field in ["from", "to"] {
                if let Some(bound) = optional(options, field) {
                    out.insert(field.into(), finite(&bound, field)?.into());
                }
            }
            if let Some(limit) = limit(options.get("limit"))? {
                out.insert("limit".into(), limit);
            }
            one(command("fetchOrderHistory", out, false))
        }
        _ => Err(refused(
            "UnsupportedMethod",
            format!("{method} is not supported on the Rust exchange server"),
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn caps() -> Capabilities {
        Capabilities::BASE
    }

    fn single(method: &str, args: Value, capabilities: &Capabilities) -> Value {
        let Value::Array(args) = args else { panic!() };
        match translate(method, &args, capabilities, &[], false)
            .unwrap()
            .as_slice()
        {
            [Plan::Command { command, .. }] => command.clone(),
            other => panic!("{other:?}"),
        }
    }

    #[test]
    fn place_order_becomes_a_one_order_batch_like_rust_exchange_ts() {
        let order =
            json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":0.01,"price":50000});
        assert_eq!(
            single("placeOrder", json!([order.clone()]), &caps()),
            json!({"op":"placeOrders","orders":[order]})
        );
        let with_id = single(
            "placeOrderWithClientOrderId",
            json!([order, "abc"]),
            &caps(),
        );
        assert_eq!(with_id["orders"][0]["clientOrderId"], "abc");
    }

    #[test]
    fn account_and_credential_fields_never_reach_a_command() {
        for field in ["accountName", "key", "secret", "passphrase"] {
            let mut order = json!({"symbol":"BTCUSDT","amount":1});
            order[field] = "x".into();
            let refusal =
                translate("placeOrders", &[json!([order])], &caps(), &[], false).unwrap_err();
            assert!(refusal.message.contains(field), "{field}");
        }
    }

    #[test]
    fn non_finite_and_oversized_arguments_refuse_before_the_engine() {
        let order = json!({"symbol":"BTCUSDT","amount":"1e999"});
        assert!(translate("placeOrder", &[order], &caps(), &[], false).is_err());
        let huge = json!([{"symbol": "x".repeat(MAX_ARGS_BYTES)}]);
        let refusal = translate("placeOrders", &[huge], &caps(), &[], false).unwrap_err();
        assert_eq!(refusal.name, "RangeError");
    }

    #[test]
    fn amend_and_cancel_follow_engine_capabilities() {
        let request = json!([{"order":{"id":"1","symbol":"BTCUSDT"},"update":{"price":10}}]);
        assert_eq!(
            single("updateOrder", request.clone(), &caps())["op"],
            "amendOrder"
        );
        let with_symbol = Capabilities {
            amend_with_symbol: true,
            cancel_with_symbols: true,
            ..caps()
        };
        let amend = single("updateOrder", request, &with_symbol);
        assert_eq!(
            amend,
            json!({"op":"amendOrderWithSymbol","id":"1","symbol":"BTCUSDT","price":10.0})
        );
        let orders = json!([[{"id":"1","symbol":"BTCUSDT","type":"limit"}]]);
        assert_eq!(
            single("cancelOrders", orders.clone(), &caps())["ids"],
            json!(["1"])
        );
        assert_eq!(
            single("cancelOrders", orders, &with_symbol)["op"],
            "cancelOrdersWithSymbols"
        );
    }

    #[test]
    fn cancel_all_is_native_or_one_command_per_open_symbol() {
        let native = Capabilities {
            cancel_all: CancelAll::Native,
            ..caps()
        };
        assert_eq!(
            single("cancelAllOrders", json!([]), &native),
            json!({"op":"cancelSymbolOrders","symbol":""})
        );
        let each = Capabilities {
            cancel_all: CancelAll::EachOpenSymbol,
            ..caps()
        };
        let symbols = ["ETH".to_owned(), "BTC".to_owned(), "ETH".to_owned()];
        let plans = translate("cancelAllOrders", &[], &each, &symbols, false).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(
            translate("cancelAllOrders", &[], &each, &[], false).unwrap(),
            vec![Plan::Reply(Value::Null)]
        );
    }

    #[test]
    fn unknown_methods_are_refused_not_guessed() {
        let refusal = translate("withdraw", &[], &caps(), &[], false).unwrap_err();
        assert_eq!(refusal.name, "UnsupportedMethod");
    }

    #[test]
    fn symbol_metadata_reads_what_the_browser_engine_reads() {
        let symbol = json!(["BTCUSDT"]);
        let quiet = translate(
            "updateSymbolMetadata",
            &[json!("BTCUSDT")],
            &caps(),
            &[],
            false,
        );
        assert_eq!(quiet.unwrap(), vec![Plan::Reply(Value::Null)]);
        let query = Capabilities {
            max_leverage: MaxLeverage::Query,
            ..caps()
        };
        assert_eq!(
            single("updateSymbolMetadata", symbol.clone(), &query),
            json!({"op":"fetchMaxLeverage","symbol":"BTCUSDT"})
        );
        for leverage_query in [LeverageQuery::OnMetadata, LeverageQuery::OnReadAndMetadata] {
            let rails = Capabilities {
                leverage_query,
                ..caps()
            };
            assert_eq!(
                single("updateSymbolMetadata", symbol.clone(), &rails),
                json!({"op":"fetchLeverage","symbol":"BTCUSDT"})
            );
            let public = translate(
                "updateSymbolMetadata",
                &[json!("BTCUSDT")],
                &rails,
                &[],
                true,
            );
            assert_eq!(public.unwrap(), vec![Plan::Reply(Value::Null)]);
        }
    }

    #[test]
    fn history_reads_become_engine_queries_with_integer_limits() {
        let history = Capabilities {
            fill_history: true,
            order_history: true,
            fill_history_time: FillHistoryTime::SecondsOrMilliseconds,
            ..caps()
        };
        assert_eq!(
            single(
                "fetchHistoricalFills",
                json!([{"symbol":"BTCUSDT","from":1_700_000_000,"to":1_700_000_100_000_u64,"limit":100}]),
                &history,
            ),
            json!({"op":"fetchFills","symbol":"BTCUSDT","from":1_700_000_000_000.0,
                "to":1_700_000_100_000.0,"limit":100})
        );
        let command = single(
            "fetchHistoricalOrders",
            json!([{"from":1.0,"to":2.0,"limit":50.0}]),
            &history,
        );
        // The engine reads `limit` as u32: it must go out as 50, never 50.0.
        assert_eq!(
            command,
            json!({"op":"fetchOrderHistory","from":1.0,"to":2.0,"limit":50})
        );
        assert_eq!(command["limit"].to_string(), "50");
        for limit in [json!(1.5), json!(-1), json!("10")] {
            let refused = translate(
                "fetchHistoricalOrders",
                &[json!({ "limit": limit })],
                &history,
                &[],
                false,
            );
            assert_eq!(refused.unwrap_err().name, "TypeError", "{limit}");
        }
        let without = Capabilities {
            fill_history: false,
            ..caps()
        };
        let without = translate("fetchHistoricalFills", &[json!({})], &without, &[], false);
        assert_eq!(without.unwrap_err().name, "UnsupportedMethod");
        let strict = Capabilities {
            reject_order_history_filters: true,
            ..history
        };
        let filtered = translate(
            "fetchHistoricalOrders",
            &[json!({"orderId":"1"})],
            &strict,
            &[],
            false,
        );
        assert_eq!(filtered.unwrap_err().name, "Error");
    }
}
