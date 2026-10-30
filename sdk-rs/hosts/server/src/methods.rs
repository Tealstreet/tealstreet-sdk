//! `callMethod {method, args}` to the engine command JSON that `RustEngineExchange`
//! (safe-cex `rust.exchange.ts`) sends to `WasmEngine.command`. Every produced
//! command still passes the engine's typed decoder; an unknown method never
//! gets a guessed command.
use safe_cex_engine::Capabilities;
use safe_cex_engine::capabilities::CancelAll;
use serde_json::{Map, Value, json};

/// Client `args` larger than this are refused before translation.
pub const MAX_ARGS_BYTES: usize = 64 * 1024;

/// What a method becomes: one engine command, or a direct reply.
#[derive(Debug, PartialEq)]
pub enum Plan {
    Command {
        command: Value,
        reply: Reply,
    },
    /// Answered without the engine (`cancelAllOrders` with nothing to cancel).
    Reply(Value),
}

/// What the method returns once its command is done.
#[derive(Clone, Debug, PartialEq)]
pub enum Reply {
    /// The engine result as is.
    Result,
    /// The TS method returns `undefined`, not the engine result.
    Unit,
    /// `placeOrderWithClientOrderId`: `{orderIds, clientOrderId}`, as base.ts.
    Placed { client_order_id: String },
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
        reply: if returns_unit {
            Reply::Unit
        } else {
            Reply::Result
        },
    }
}

fn fields(pairs: impl IntoIterator<Item = (&'static str, Value)>) -> Map<String, Value> {
    pairs
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect()
}

/// Translates one hosted `callMethod`. `open_order_symbols` (engine store) answers
/// cancel-all for venues whose engine cancels one symbol at a time.
pub fn translate(
    method: &str,
    args: &[Value],
    capabilities: &Capabilities,
    open_order_symbols: &[String],
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
        "placeOrder" => {
            let order = order(arg(args, 0))?;
            one(command(
                "placeOrders",
                fields([("orders", json!([order]))]),
                false,
            ))
        }
        // As base.ts: the ID rides inside the one order argument, and a call
        // without one never places an untracked order in its name.
        "placeOrderWithClientOrderId" => {
            let order = order(arg(args, 0))?;
            let Some(id) = order["clientOrderId"].as_str().map(str::to_owned) else {
                return Err(refused(
                    "Error",
                    "placeOrderWithClientOrderId requires a clientOrderId",
                ));
            };
            one(Plan::Command {
                command: json!({"op": "placeOrders", "orders": [order]}),
                reply: Reply::Placed {
                    client_order_id: id,
                },
            })
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
        match translate(method, &args, capabilities, &[])
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
    }

    #[test]
    fn place_order_with_client_order_id_reads_the_id_from_the_order_like_base_ts() {
        // The web's sendOrder: `placeOrderWithClientOrderId({...order, clientOrderId})`.
        let order = json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":0.001,
            "price":60000,"postOnly":true,"clientOrderId":"1730250000000-abc"});
        for args in [json!([order.clone()]), json!([order.clone(), null])] {
            let Value::Array(args) = args else { panic!() };
            let plans = translate("placeOrderWithClientOrderId", &args, &caps(), &[]).unwrap();
            assert_eq!(
                plans,
                vec![Plan::Command {
                    command: json!({"op":"placeOrders","orders":[order.clone()]}),
                    reply: Reply::Placed {
                        client_order_id: "1730250000000-abc".into()
                    },
                }]
            );
        }
        let mut without = order.clone();
        without.as_object_mut().unwrap().remove("clientOrderId");
        for args in [vec![without.clone()], vec![without, json!("abc")]] {
            let refusal =
                translate("placeOrderWithClientOrderId", &args, &caps(), &[]).unwrap_err();
            assert!(refusal.message.contains("requires a clientOrderId"));
        }
    }

    #[test]
    fn account_and_credential_fields_never_reach_a_command() {
        for field in ["accountName", "key", "secret", "passphrase"] {
            let mut order = json!({"symbol":"BTCUSDT","amount":1});
            order[field] = "x".into();
            let refusal = translate("placeOrders", &[json!([order])], &caps(), &[]).unwrap_err();
            assert!(refusal.message.contains(field), "{field}");
        }
    }

    #[test]
    fn non_finite_and_oversized_arguments_refuse_before_the_engine() {
        let order = json!({"symbol":"BTCUSDT","amount":"1e999"});
        assert!(translate("placeOrder", &[order], &caps(), &[]).is_err());
        let huge = json!([{"symbol": "x".repeat(MAX_ARGS_BYTES)}]);
        let refusal = translate("placeOrders", &[huge], &caps(), &[]).unwrap_err();
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
        let plans = translate("cancelAllOrders", &[], &each, &symbols).unwrap();
        assert_eq!(plans.len(), 2);
        assert_eq!(
            translate("cancelAllOrders", &[], &each, &[]).unwrap(),
            vec![Plan::Reply(Value::Null)]
        );
    }

    #[test]
    fn unknown_methods_are_refused_not_guessed() {
        let refusal = translate("withdraw", &[], &caps(), &[]).unwrap_err();
        assert_eq!(refusal.name, "UnsupportedMethod");
    }
}
