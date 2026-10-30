//! One info line per order-affecting command, so "who placed this order" is
//! answerable from the journal. Every field comes from an allowlist: keys,
//! signatures and proxy tokens can never reach a line.
use safe_cex_engine::engine::Command;
use safe_cex_engine::error::VenueError;
use serde_json::{Map, Value, json};

/// The account a command acts on.
#[derive(Clone, Debug, Default)]
pub struct Who {
    pub user: String,
    pub account: String,
    pub venue: String,
}

const ENGINE_OPS: [&str; 10] = [
    "placeOrders",
    "cancelOrders",
    "cancelOrdersWithSymbols",
    "cancelSymbolOrders",
    "amendOrder",
    "amendOrderWithSymbol",
    "setLeverage",
    "setMarginType",
    "setPositionMode",
    "setStpMode",
];

/// Hosted `callMethod`s that change orders or account settings.
pub const ORDER_METHODS: [&str; 11] = [
    "placeOrders",
    "placeOrder",
    "placeOrderWithClientOrderId",
    "updateOrder",
    "cancelOrders",
    "cancelSymbolOrders",
    "cancelAllOrders",
    "setLeverage",
    "setAllLeverage",
    "changeMarginType",
    "changePositionMode",
];

const COMMAND_FIELDS: [&str; 13] = [
    "symbol",
    "side",
    "type",
    "amount",
    "price",
    "triggerPrice",
    "reduceOnly",
    "clientOrderId",
    "id",
    "leverage",
    "isolated",
    "hedged",
    "mode",
];

const ALGO_FIELDS: [&str; 13] = [
    "id",
    "twapId",
    "chaserId",
    "clientId",
    "symbol",
    "side",
    "size",
    "amount",
    "price",
    "reduceOnly",
    "maxAttempts",
    "duration",
    "command",
];

const ERROR_FIELDS: [&str; 5] = ["name", "kind", "message", "outcome", "venueCode"];

const MAX_TEXT: usize = 160;

fn scalar(value: &Value) -> Option<Value> {
    match value {
        Value::String(text) => Some(Value::String(text.chars().take(MAX_TEXT).collect())),
        Value::Number(_) | Value::Bool(_) => Some(value.clone()),
        _ => None,
    }
}

fn pick(source: &Value, fields: &[&str]) -> Map<String, Value> {
    fields
        .iter()
        .filter_map(|field| Some(((*field).to_owned(), scalar(source.get(*field)?)?)))
        .collect()
}

/// The audited fields of an engine command (as JSON), or `None` for a read.
pub fn command(command: &Value) -> Option<Map<String, Value>> {
    let op = command["op"]
        .as_str()
        .filter(|op| ENGINE_OPS.contains(op))?;
    let mut fields = pick(command, &COMMAND_FIELDS);
    fields.insert("op".into(), op.into());
    if let Some(ids) = command["ids"].as_array() {
        fields.insert("ids".into(), ids.iter().filter_map(scalar).collect());
    }
    if let Some(orders) = command["orders"].as_array() {
        let orders = orders
            .iter()
            .map(|order| Value::Object(pick(order, &COMMAND_FIELDS)))
            .collect();
        fields.insert("orders".into(), Value::Array(orders));
    }
    Some(fields)
}

/// The audited fields of a hosted call refused before it became a command.
pub fn refused_call(method: &str, args: &[Value]) -> Map<String, Value> {
    let mut fields = match args.first() {
        Some(Value::Array(orders)) => [(
            "orders".to_owned(),
            orders
                .iter()
                .map(|order| Value::Object(pick(order, &COMMAND_FIELDS)))
                .collect(),
        )]
        .into_iter()
        .collect(),
        Some(first) => pick(first, &COMMAND_FIELDS),
        None => Map::new(),
    };
    fields.insert("op".into(), method.into());
    fields
}

/// The audited fields of a typed engine command an algo sent.
pub fn typed(command: &Command) -> Option<Map<String, Value>> {
    let json = match command {
        Command::PlaceOrders { orders, .. } => json!({"op": "placeOrders", "orders": orders}),
        Command::CancelOrders { ids, .. } => json!({"op": "cancelOrders", "ids": ids}),
        Command::CancelOrdersWithSymbols { orders, .. } => {
            let orders: Vec<Value> = orders
                .iter()
                .map(|order| json!({"id": order.id, "symbol": order.symbol}))
                .collect();
            json!({"op": "cancelOrdersWithSymbols", "orders": orders})
        }
        Command::AmendOrder {
            id, price, amount, ..
        } => json!({"op": "amendOrder", "id": id, "price": price, "amount": amount}),
        Command::AmendOrderWithSymbol {
            id,
            symbol,
            price,
            amount,
            ..
        } => {
            json!({"op": "amendOrderWithSymbol", "id": id, "symbol": symbol, "price": price, "amount": amount})
        }
        Command::CancelSymbolOrders { symbol, .. } => {
            json!({"op": "cancelSymbolOrders", "symbol": symbol})
        }
        Command::SetLeverage {
            symbol,
            leverage,
            side,
            ..
        } => json!({"op": "setLeverage", "symbol": symbol, "leverage": leverage, "side": side}),
        Command::SetMarginType {
            symbol, isolated, ..
        } => json!({"op": "setMarginType", "symbol": symbol, "isolated": isolated}),
        Command::SetPositionMode { hedged, symbol, .. } => {
            json!({"op": "setPositionMode", "hedged": hedged, "symbol": symbol})
        }
        Command::SetStpMode { mode, .. } => json!({"op": "setStpMode", "mode": mode}),
        _ => return None,
    };
    self::command(&json)
}

/// The audited fields of an algo op (`chaser:start`, `twap:stop`, `exec`, ...).
pub fn algo_op(op: &str, data: &str) -> Map<String, Value> {
    let data: Value = serde_json::from_str(data).unwrap_or(Value::Null);
    let mut fields = match &data {
        Value::String(_) => [("id".to_owned(), scalar(&data).unwrap_or_default())]
            .into_iter()
            .collect(),
        _ => pick(&data, &ALGO_FIELDS),
    };
    fields.insert("op".into(), op.into());
    fields
}

/// `ok`, or the error's kind, message and outcome from a hosted failure detail.
pub fn result(outcome: &Result<Value, Value>) -> Value {
    match outcome {
        Ok(_) => "ok".into(),
        Err(detail) => Value::Object(pick(detail, &ERROR_FIELDS)),
    }
}

pub fn venue_result<T>(outcome: &Result<T, VenueError>) -> Value {
    match outcome {
        Ok(_) => "ok".into(),
        Err(error) => {
            let detail = serde_json::to_value(error).unwrap_or_default();
            Value::Object(pick(&detail, &ERROR_FIELDS))
        }
    }
}

/// The full line: who, where it came from (`socket:N/client:M`, `algo`, ...), what, and the result.
pub fn line(who: &Who, origin: &str, fields: &Map<String, Value>, result: Value) -> Value {
    let mut line = json!({
        "level": "info",
        "service": "exchange-server",
        "msg": "order audit",
        "user": who.user,
        "account": who.account,
        "venue": who.venue,
        "origin": origin,
    });
    for (key, value) in fields {
        line[key] = value.clone();
    }
    line["result"] = result;
    line
}

pub fn log(who: &Who, origin: &str, fields: &Map<String, Value>, result: Value) {
    println!("{}", line(who, origin, fields, result));
}

#[cfg(test)]
mod tests {
    use super::*;

    fn who() -> Who {
        Who {
            user: "user-1".into(),
            account: "bear5".into(),
            venue: "bybit".into(),
        }
    }

    #[test]
    fn a_placement_line_carries_who_what_where_and_the_result() {
        let command = json!({"op": "placeOrders", "command": 7, "orders": [{
            "symbol": "BTCUSDT", "side": "buy", "type": "limit", "amount": 0.01,
            "price": 60000.5, "clientOrderId": "ts-abc", "key": "pub-key", "secret": "s"
        }]});
        let fields = super::command(&command).unwrap();
        let line = line(
            &who(),
            "socket:3/client:9",
            &fields,
            result(&Ok(Value::Null)),
        );
        assert_eq!(line["msg"], "order audit");
        assert_eq!(line["level"], "info");
        assert_eq!(line["user"], "user-1");
        assert_eq!(line["account"], "bear5");
        assert_eq!(line["venue"], "bybit");
        assert_eq!(line["origin"], "socket:3/client:9");
        assert_eq!(line["op"], "placeOrders");
        assert_eq!(line["result"], "ok");
        let order = &line["orders"][0];
        assert_eq!(order["symbol"], "BTCUSDT");
        assert_eq!(order["side"], "buy");
        assert_eq!(order["amount"], 0.01);
        assert_eq!(order["price"], 60000.5);
        assert_eq!(order["clientOrderId"], "ts-abc");
        let text = line.to_string();
        assert!(!text.contains("pub-key") && !text.contains("secret"));
    }

    #[test]
    fn a_failure_carries_its_kind_and_outcome_but_reads_are_not_audited() {
        let fields =
            super::command(&json!({"op": "setLeverage", "symbol": "ETHUSDT", "leverage": 5.0}))
                .unwrap();
        let failed = Err(
            json!({"name": "ExchangeError", "message": "leverage not modified", "outcome": "unknown", "signature": "sig"}),
        );
        let line = line(&who(), "socket:1/client:2", &fields, result(&failed));
        assert_eq!(line["leverage"], 5.0);
        assert_eq!(line["result"]["name"], "ExchangeError");
        assert_eq!(line["result"]["outcome"], "unknown");
        assert!(line["result"].get("signature").is_none());
        assert!(super::command(&json!({"op": "fetchAccount", "command": 1})).is_none());
    }

    #[test]
    fn a_refused_call_still_logs_what_was_asked() {
        let order = json!({"symbol": "BTCUSDT", "side": "buy", "amount": 0.001, "price": 60000, "passphrase": "p"});
        let fields = refused_call("placeOrderWithClientOrderId", &[order]);
        assert_eq!(fields["op"], "placeOrderWithClientOrderId");
        assert_eq!(fields["symbol"], "BTCUSDT");
        assert!(fields.get("passphrase").is_none());
    }

    #[test]
    fn algo_commands_and_ops_are_audited() {
        let place: Command = safe_cex_engine::engine::Command::CancelOrders {
            command: 4,
            ids: vec!["o-1".into()],
        };
        let fields = typed(&place).unwrap();
        assert_eq!(fields["op"], "cancelOrders");
        assert_eq!(fields["ids"], json!(["o-1"]));
        let start = algo_op(
            "chaser:start",
            r#"{"symbol":"BTCUSDT","size":0.5,"side":"sell","maxAttempts":10,"clientId":"c-1","accountName":"x"}"#,
        );
        assert_eq!(start["op"], "chaser:start");
        assert_eq!(start["symbol"], "BTCUSDT");
        assert_eq!(start["size"], 0.5);
        assert_eq!(start["clientId"], "c-1");
        assert!(start.get("accountName").is_none());
        assert_eq!(algo_op("chaser:terminate", r#""chase-7""#)["id"], "chase-7");
    }
}
