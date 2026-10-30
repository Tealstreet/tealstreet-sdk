//! `callMethod {method, args}` to the engine command JSON that `RustEngineExchange`
//! (safe-cex `rust.exchange.ts`) sends to `WasmEngine.command`. Every produced
//! command still passes the engine's typed decoder; an unknown method never
//! gets a guessed command.
use safe_cex_engine::capabilities::{AccountLookup, CancelAll, FillHistoryTime};
use safe_cex_engine::{Capabilities, Store};
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
    /// Answered from the account's engine store, as `rust.exchange.ts` reads its own.
    Read(StoreRead),
}

#[derive(Debug, PartialEq)]
pub enum StoreRead {
    Orders(Option<Vec<String>>),
    Positions(Option<Vec<String>>),
    Balance,
    PositionMode,
    /// `findOrderByClientOrderId` where the venue has no lookup: open orders only.
    ClientOrder {
        symbol: String,
        client_order_id: String,
    },
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
    /// `updateOrder`: the worker bridge's `WorkerOrderIds`, which the main-thread
    /// mirror decodes (`decodeWorkerOrderIds`); a bare array would throw there.
    OrderIds,
}

/// The engine's amend result (`string[]` or `{orderIds, clientOrderId}`) as `WorkerOrderIds`.
pub fn worker_order_ids(result: Value) -> Value {
    match result {
        Value::Array(ids) => json!({"orderIds": ids}),
        Value::Object(mut fields) => {
            if fields.get("clientOrderId").is_some_and(Value::is_null) {
                fields.remove("clientOrderId");
            }
            Value::Object(fields)
        }
        other => json!({"orderIds": other}),
    }
}

/// Answers a store read; reads wait for nothing, so an unloaded section refuses.
pub fn read_store(read: &StoreRead, store: &Store) -> Result<Value, Refused> {
    let loaded = store.loaded();
    let ready = |section: bool, what: &str| {
        if loaded.markets && section {
            Ok(())
        } else {
            Err(refused("Error", format!("{what} are not loaded yet")))
        }
    };
    let only = |symbols: &Option<Vec<String>>, symbol: &str| {
        symbols
            .as_ref()
            .is_none_or(|symbols| symbols.iter().any(|wanted| wanted == symbol))
    };
    let value = match read {
        StoreRead::Orders(symbols) => {
            ready(loaded.orders, "orders")?;
            let orders: Vec<_> = store
                .orders()
                .iter()
                .filter(|order| only(symbols, &order.symbol))
                .collect();
            serde_json::to_value(orders)
        }
        StoreRead::Positions(symbols) => {
            ready(loaded.positions, "positions")?;
            let positions: Vec<_> = store
                .positions()
                .iter()
                .filter(|position| only(symbols, &position.symbol))
                .collect();
            serde_json::to_value(positions)
        }
        StoreRead::Balance => {
            ready(loaded.balance, "balances")?;
            serde_json::to_value(store.balance())
        }
        StoreRead::PositionMode => {
            ready(loaded.positions, "positions")?;
            Ok(store.hedged().into())
        }
        StoreRead::ClientOrder {
            symbol,
            client_order_id,
        } => {
            let order = store.orders().iter().find(|order| {
                &order.symbol == symbol && order.client_order_id.as_ref() == Some(client_order_id)
            });
            Ok(match order {
                Some(order) => json!({"found": true, "order": order}),
                None => json!({"found": false, "definitive": false}),
            })
        }
    };
    value.map_err(|error| refused("Error", error.to_string()))
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

/// `value`'s field when present and not null (JSON turns `undefined` into absence or null).
fn present<'a>(value: &'a Value, field: &str) -> Option<&'a Value> {
    value.get(field).filter(|value| !value.is_null())
}

/// Copies the present `(from, to)` fields of `source`, as the TS command spreads them.
fn optional(out: &mut Map<String, Value>, source: &Value, pairs: &[(&'static str, &'static str)]) {
    for (from, to) in pairs {
        if let Some(value) = present(source, from) {
            out.insert((*to).into(), value.clone());
        }
    }
}

fn symbols(value: &Value) -> Result<Option<Vec<String>>, Refused> {
    match value {
        Value::Null => Ok(None),
        Value::Array(symbols) => symbols
            .iter()
            .map(|symbol| text(symbol, "symbol"))
            .collect::<Result<_, _>>()
            .map(Some),
        _ => Err(refused("TypeError", "symbols must be an array")),
    }
}

/// `fetchHistoricalFills` bounds, as `rust.exchange.ts` `milliseconds`.
fn fill_bound(value: Option<&Value>, nonnegative: bool) -> Option<Value> {
    let value = value?.as_f64().filter(|value| value.is_finite())?;
    if nonnegative && value < 0.0 {
        return None;
    }
    let already = if nonnegative {
        value >= 1_000_000_000_000.0
    } else {
        value > 1_000_000_000_000.0
    };
    Some(
        (if already { value } else { value * 1000.0 })
            .floor()
            .into(),
    )
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
            out.insert("op".into(), op.into());
            one(Plan::Command {
                command: Value::Object(out),
                reply: Reply::OrderIds,
            })
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
        "findOrderByClientOrderId" => {
            let symbol = text(arg(args, 0), "symbol")?;
            let id = text(arg(args, 1), "clientOrderId")?;
            if !capabilities.client_order_lookup {
                return one(Plan::Read(StoreRead::ClientOrder {
                    symbol,
                    client_order_id: id,
                }));
            }
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
            // As `!symbol` in rust.exchange.ts: an empty symbol is no symbol.
            let symbol = match arg(args, 1) {
                Value::Null => String::new(),
                symbol => text(symbol, "symbol")?,
            };
            if symbol.is_empty() && !capabilities.account_margin_type {
                return Err(refused("Error", "Symbol is required"));
            }
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
        // A main-thread mirror learns the record here (`loadWorkerCapabilities`).
        "fetchEngineCapabilities" => one(Plan::Reply(
            serde_json::to_value(capabilities)
                .map_err(|error| refused("Error", error.to_string()))?,
        )),
        "getAccount" => match capabilities.account {
            AccountLookup::Unsupported => Err(refused("Error", "Not implemented")),
            AccountLookup::Query | AccountLookup::AfterLoad => {
                one(command("fetchAccount", Map::new(), false))
            }
        },
        "fetchOrders" => one(Plan::Read(StoreRead::Orders(symbols(arg(args, 0))?))),
        "fetchPositions" => one(Plan::Read(StoreRead::Positions(symbols(arg(args, 0))?))),
        "fetchBalance" => one(Plan::Read(StoreRead::Balance)),
        "fetchPositionMode" => one(Plan::Read(StoreRead::PositionMode)),
        "fetchOHLCV" => {
            let opts = arg(args, 0);
            let mut out = fields([
                ("symbol", text(&opts["symbol"], "symbol")?.into()),
                ("interval", text(&opts["interval"], "interval")?.into()),
            ]);
            optional(
                &mut out,
                opts,
                &[("limit", "limit"), ("from", "from"), ("to", "to")],
            );
            one(command("fetchCandles", out, false))
        }
        "fetchPublicTrades" => {
            let mut out = fields([("symbol", text(arg(args, 0), "symbol")?.into())]);
            if let Some(limit) = args.get(1).filter(|limit| !limit.is_null()) {
                out.insert("limit".into(), limit.clone());
            }
            one(command("fetchPublicTrades", out, false))
        }
        "fetchHistoricalFills" => {
            let opts = arg(args, 0);
            let bounds = [present(opts, "from"), present(opts, "to")];
            if capabilities.finite_history_bounds
                && bounds
                    .iter()
                    .flatten()
                    .any(|bound| !bound.as_f64().is_some_and(f64::is_finite))
            {
                return Err(refused("Error", "Coinbase history bounds must be finite"));
            }
            let mut out = Map::new();
            optional(
                &mut out,
                opts,
                &[
                    ("symbol", "symbol"),
                    ("orderId", "orderId"),
                    ("limit", "limit"),
                ],
            );
            match capabilities.fill_history_time {
                FillHistoryTime::AsProvided => {
                    optional(&mut out, opts, &[("from", "from"), ("to", "to")])
                }
                time => {
                    let nonnegative = time == FillHistoryTime::NonnegativeSecondsOrMilliseconds;
                    for (field, bound) in ["from", "to"].into_iter().zip(bounds) {
                        if let Some(bound) = fill_bound(bound, nonnegative) {
                            out.insert(field.into(), bound);
                        }
                    }
                }
            }
            one(command("fetchFills", out, false))
        }
        "fetchHistoricalOrders" => {
            let opts = arg(args, 0);
            if capabilities.reject_order_history_filters
                && (present(opts, "orderId").is_some() || present(opts, "status").is_some())
            {
                return Err(refused(
                    "Error",
                    "order history does not support orderId or status filters",
                ));
            }
            let mut out = Map::new();
            optional(
                &mut out,
                opts,
                &[
                    ("symbol", "symbol"),
                    ("from", "from"),
                    ("to", "to"),
                    ("limit", "limit"),
                ],
            );
            one(command("fetchOrderHistory", out, false))
        }
        "fetchIncomeHistory" => {
            let opts = arg(args, 0);
            let mut out = Map::new();
            optional(
                &mut out,
                opts,
                &[
                    ("symbol", "symbol"),
                    ("incomeType", "incomeType"),
                    ("startTime", "from"),
                    ("endTime", "to"),
                    ("limit", "limit"),
                ],
            );
            one(command("fetchIncome", out, false))
        }
        "fetchIncomeHistoryRange" => {
            let mut out = fields([
                ("from", finite(arg(args, 0), "startTime")?.into()),
                ("to", finite(arg(args, 1), "endTime")?.into()),
            ]);
            if let Some(kind) = args.get(2).filter(|kind| !kind.is_null()) {
                out.insert("incomeType".into(), text(kind, "incomeType")?.into());
            }
            one(command("fetchIncome", out, false))
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

    /// Translates the web's real `args` and proves the engine's typed decoder takes the command.
    fn web(method: &str, args: Value, capabilities: &Capabilities) -> (Value, Reply) {
        let Value::Array(args) = args else { panic!() };
        let plans = translate(method, &args, capabilities, &[]).unwrap();
        let [Plan::Command { command, reply }] = plans.as_slice() else {
            panic!("{method}: {plans:?}")
        };
        let mut wire = command.clone();
        wire["command"] = 1.into();
        safe_cex_engine::command::decode_host_input(&wire.to_string())
            .unwrap_or_else(|error| panic!("{method}: {wire} -> {error}"));
        (command.clone(), reply.clone())
    }

    fn read(method: &str, args: Value, capabilities: &Capabilities) -> StoreRead {
        let Value::Array(args) = args else { panic!() };
        match translate(method, &args, capabilities, &[]).unwrap().pop() {
            Some(Plan::Read(read)) => read,
            other => panic!("{method}: {other:?}"),
        }
    }

    // Web: order form / ladder / hotkeys `sendOrder` -> placeOrderWithClientOrderId({...order, clientOrderId}).
    #[test]
    fn web_place_order_with_client_order_id_decodes() {
        let order = json!({"symbol":"BTCUSDT","side":"buy","type":"limit","amount":0.001,"price":60000,
            "reduceOnly":false,"timeInForce":"PostOnly","clientOrderId":"ts-1730250000000-abc"});
        let (command, reply) = web(
            "placeOrderWithClientOrderId",
            json!([order.clone()]),
            &caps(),
        );
        assert_eq!(command, json!({"op":"placeOrders","orders":[order]}));
        assert_eq!(
            reply,
            Reply::Placed {
                client_order_id: "ts-1730250000000-abc".into()
            }
        );
    }

    // Web: createScaledOrder / TP-SL protective orders -> placeOrders(orders).
    #[test]
    fn web_place_orders_with_tp_sl_decodes() {
        let orders = json!([
            {"symbol":"BTCUSDT","side":"buy","type":"limit","amount":0.001,"price":60000,"reduceOnly":false},
            {"symbol":"BTCUSDT","side":"sell","type":"stop_market","amount":0.001,"price":55000,"reduceOnly":true},
            {"symbol":"BTCUSDT","side":"sell","type":"take_profit_market","amount":0.001,"price":70000,"reduceOnly":true},
        ]);
        let (command, reply) = web("placeOrders", json!([orders.clone()]), &caps());
        assert_eq!(command, json!({"op":"placeOrders","orders":orders}));
        assert_eq!(reply, Reply::Result);
        let market = json!({"symbol":"BTCUSDT","side":"sell","type":"market","amount":0.001,"reduceOnly":true});
        let (command, _) = web("placeOrder", json!([market.clone()]), &caps());
        assert_eq!(command, json!({"op":"placeOrders","orders":[market]}));
    }

    // Web: updateOrder.ts sends the whole order (with info) and a price or amount update.
    #[test]
    fn web_update_order_decodes_and_replies_worker_order_ids() {
        let request = json!([{"order":{"id":"o-1","symbol":"BTCUSDT","type":"limit","side":"buy","price":60000,
            "amount":0.001,"remaining":0.001,"filled":0,"status":"open","reduceOnly":false,"info":{"x":1}},
            "update":{"price":59000}}]);
        let (command, reply) = web("updateOrder", request.clone(), &caps());
        assert_eq!(
            command,
            json!({"op":"amendOrder","id":"o-1","price":59000.0})
        );
        assert_eq!(reply, Reply::OrderIds);
        let with_symbol = Capabilities {
            amend_with_symbol: true,
            ..caps()
        };
        let (command, _) = web("updateOrder", request, &with_symbol);
        assert_eq!(command["symbol"], "BTCUSDT");
        assert_eq!(
            worker_order_ids(json!(["o-2"])),
            json!({"orderIds":["o-2"]})
        );
        assert_eq!(
            worker_order_ids(json!({"orderIds":["o-2"],"clientOrderId":"c"})),
            json!({"orderIds":["o-2"],"clientOrderId":"c"})
        );
        assert_eq!(
            worker_order_ids(json!({"orderIds":["o-2"],"clientOrderId":null})),
            json!({"orderIds":["o-2"]})
        );
    }

    // Web: cancelOrders.ts / cancel hotkeys pass store orders, some without a type.
    #[test]
    fn web_cancel_orders_decodes_with_and_without_type() {
        let orders = json!([[{"id":"o-1","symbol":"BTCUSDT","type":"limit","side":"buy","price":1,"amount":1},
            {"id":"o-2","symbol":"ETHUSDT","side":"sell","price":1,"amount":1}]]);
        let (command, reply) = web("cancelOrders", orders.clone(), &caps());
        assert_eq!(command["ids"], json!(["o-1", "o-2"]));
        assert_eq!(reply, Reply::Unit);
        let with_symbols = Capabilities {
            cancel_with_symbols: true,
            ..caps()
        };
        web("cancelOrders", orders, &with_symbols);
        web("cancelSymbolOrders", json!(["BTCUSDT"]), &caps());
    }

    #[test]
    fn web_cancel_all_orders_decodes() {
        let native = Capabilities {
            cancel_all: CancelAll::Native,
            ..caps()
        };
        web("cancelAllOrders", json!([]), &native);
    }

    // Web: lostSendResolver -> findOrderByClientOrderId(symbol, clientOrderId).
    #[test]
    fn web_find_order_by_client_order_id_asks_the_venue_or_the_store() {
        let lookup = Capabilities {
            client_order_lookup: true,
            ..caps()
        };
        let (command, _) = web(
            "findOrderByClientOrderId",
            json!(["BTCUSDT", "c-1"]),
            &lookup,
        );
        assert_eq!(command["clientOrderId"], "c-1");
        assert_eq!(
            read(
                "findOrderByClientOrderId",
                json!(["BTCUSDT", "c-1"]),
                &caps()
            ),
            StoreRead::ClientOrder {
                symbol: "BTCUSDT".into(),
                client_order_id: "c-1".into()
            }
        );
    }

    // Web: setLeverage.ts -> setLeverage(symbol, leverage, side); `side` undefined arrives as null.
    #[test]
    fn web_set_leverage_decodes_with_and_without_side() {
        let (command, reply) = web("setLeverage", json!(["BTCUSDT", 10, null]), &caps());
        assert_eq!(
            command,
            json!({"op":"setLeverage","symbol":"BTCUSDT","leverage":10.0})
        );
        assert_eq!(reply, Reply::Unit);
        let (command, _) = web("setLeverage", json!(["BTCUSDT", 10, "long"]), &caps());
        assert_eq!(command["side"], "long");
        let account = Capabilities {
            account_leverage: true,
            ..caps()
        };
        web("setAllLeverage", json!([5]), &account);
    }

    // Web: useMarginType -> changeMarginType(isolated, symbol).
    #[test]
    fn web_change_margin_type_decodes_and_an_empty_symbol_needs_account_scope() {
        let (command, _) = web("changeMarginType", json!([true, "BTCUSDT"]), &caps());
        assert_eq!(
            command,
            json!({"op":"setMarginType","symbol":"BTCUSDT","isolated":true})
        );
        for symbol in [json!(null), json!("")] {
            let refusal = translate(
                "changeMarginType",
                &[json!(false), symbol.clone()],
                &caps(),
                &[],
            )
            .unwrap_err();
            assert_eq!(refusal.message, "Symbol is required");
            let account = Capabilities {
                account_margin_type: true,
                ..caps()
            };
            web("changeMarginType", json!([false, symbol]), &account);
        }
    }

    // Web: usePositionMode -> changePositionMode(hedged, symbol).
    #[test]
    fn web_change_position_mode_decodes() {
        let (command, _) = web("changePositionMode", json!([true, "BTCUSDT"]), &caps());
        assert_eq!(
            command,
            json!({"op":"setPositionMode","hedged":true,"symbol":"BTCUSDT"})
        );
        let (command, _) = web("changePositionMode", json!([false, null]), &caps());
        assert_eq!(command, json!({"op":"setPositionMode","hedged":false}));
    }

    // Mirror `loadCapabilities` and `getAccount`.
    #[test]
    fn web_capabilities_and_account_reads() {
        let Plan::Reply(record) = translate("fetchEngineCapabilities", &[], &caps(), &[])
            .unwrap()
            .remove(0)
        else {
            panic!()
        };
        assert_eq!(record, serde_json::to_value(caps()).unwrap());
        let query = Capabilities {
            account: AccountLookup::Query,
            ..caps()
        };
        web("getAccount", json!([]), &query);
        let none = Capabilities {
            account: AccountLookup::Unsupported,
            ..caps()
        };
        assert!(translate("getAccount", &[], &none, &[]).is_err());
    }

    // eagerExchangeWarmStart -> fetchOrders() / fetchPositions(); nuke and the mirror read the rest.
    #[test]
    fn web_account_reads_answer_from_the_store() {
        assert_eq!(
            read("fetchOrders", json!([]), &caps()),
            StoreRead::Orders(None)
        );
        assert_eq!(
            read("fetchPositions", json!([["BTCUSDT"]]), &caps()),
            StoreRead::Positions(Some(vec!["BTCUSDT".into()]))
        );
        assert_eq!(read("fetchBalance", json!([]), &caps()), StoreRead::Balance);
        assert_eq!(
            read("fetchPositionMode", json!([]), &caps()),
            StoreRead::PositionMode
        );
        let store = Store::default();
        let refusal = read_store(&StoreRead::Orders(None), &store).unwrap_err();
        assert!(refusal.message.contains("not loaded"));
        assert_eq!(
            read_store(
                &StoreRead::ClientOrder {
                    symbol: "BTCUSDT".into(),
                    client_order_id: "c".into()
                },
                &store
            )
            .unwrap(),
            json!({"found":false,"definitive":false})
        );
    }

    // Web: Dom.tsx / SafeCexKLinesChart -> fetchOHLCV({symbol, interval, limit?, from?, to?}).
    #[test]
    fn web_fetch_ohlcv_and_public_trades_decode() {
        let (command, _) = web(
            "fetchOHLCV",
            json!([{"symbol":"BTCUSDT","interval":"1m","limit":200}]),
            &caps(),
        );
        assert_eq!(
            command,
            json!({"op":"fetchCandles","symbol":"BTCUSDT","interval":"1m","limit":200})
        );
        web(
            "fetchOHLCV",
            json!([{"symbol":"BTCUSDT","interval":"1h","from":1_700_000_000_000u64,"to":1_700_003_600_000u64}]),
            &caps(),
        );
        let (command, _) = web("fetchPublicTrades", json!(["BTCUSDT", null]), &caps());
        assert_eq!(
            command,
            json!({"op":"fetchPublicTrades","symbol":"BTCUSDT"})
        );
        web("fetchPublicTrades", json!(["BTCUSDT", 50]), &caps());
    }

    // Web: collectAccountAxes -> fetchHistoricalFills({symbol?, from, to}) / fetchHistoricalOrders({from, to}).
    #[test]
    fn web_history_reads_decode_with_rust_exchange_ts_bounds() {
        let (command, _) = web(
            "fetchHistoricalFills",
            json!([{"symbol":"BTCUSDT","from":1_700_000_000_000u64,"to":1_700_000_100_000u64}]),
            &caps(),
        );
        assert_eq!(command["from"], 1_700_000_000_000u64);
        let seconds = Capabilities {
            fill_history_time: FillHistoryTime::SecondsOrMilliseconds,
            ..caps()
        };
        let (command, _) = web(
            "fetchHistoricalFills",
            json!([{"from":1_700_000_000,"to":1_700_000_000_000u64}]),
            &seconds,
        );
        assert_eq!(command["from"], json!(1_700_000_000_000.0));
        assert_eq!(command["to"], json!(1_700_000_000_000.0));
        let (command, _) = web("fetchHistoricalOrders", json!([{"from":1,"to":2}]), &caps());
        assert_eq!(command, json!({"op":"fetchOrderHistory","from":1,"to":2}));
        let strict = Capabilities {
            reject_order_history_filters: true,
            ..caps()
        };
        assert!(
            translate(
                "fetchHistoricalOrders",
                &[json!({"status":"filled"})],
                &strict,
                &[]
            )
            .is_err()
        );
        let (command, _) = web(
            "fetchIncomeHistory",
            json!([{"symbol":"BTCUSDT","startTime":1,"endTime":2}]),
            &caps(),
        );
        assert_eq!(
            command,
            json!({"op":"fetchIncome","symbol":"BTCUSDT","from":1,"to":2})
        );
        let (command, _) = web("fetchIncomeHistoryRange", json!([1, 2, null]), &caps());
        assert_eq!(command, json!({"op":"fetchIncome","from":1.0,"to":2.0}));
        web("fetchHistoricalFills", json!([null]), &caps());
        web("fetchHistoricalOrders", json!([]), &caps());
    }

    #[test]
    fn unknown_methods_are_refused_not_guessed() {
        let refusal = translate("withdraw", &[], &caps(), &[]).unwrap_err();
        assert_eq!(refusal.name, "UnsupportedMethod");
    }
}
