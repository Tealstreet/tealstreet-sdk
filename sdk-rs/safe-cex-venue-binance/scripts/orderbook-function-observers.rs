use crate::orderbook::{BinanceDepthUpdate, BinanceOrderBookSyncState};
use crate::protocol_types::BinanceOrderBookApplyResult;
use safe_cex_num::{JsCoercionError, number_from_string, numeric_ingress::js_value_as_f64};
use safe_cex_types::{OrderBook, OrderBookOrders};
use serde_json::{Value, json};

fn bits(value: f64) -> String {
    if value.is_nan() {
        "7ff8000000000000".into()
    } else {
        format!("{:016x}", value.to_bits())
    }
}
fn id(value: Option<f64>) -> Value {
    value.map_or_else(|| json!(["unset"]), |value| json!(["number", bits(value)]))
}
fn state(input: &Value) -> BinanceOrderBookSyncState {
    BinanceOrderBookSyncState {
        snapshot_last_update_id: input
            .get("snapshot")
            .map(|v| number_from_string(v.as_str().unwrap())),
        last_applied_update_id: input
            .get("applied")
            .map(|v| number_from_string(v.as_str().unwrap())),
    }
}
fn observed_state(state: &BinanceOrderBookSyncState) -> Value {
    json!({"snapshot":id(state.snapshot_last_update_id),"applied":id(state.last_applied_update_id)})
}
fn seed() -> OrderBook {
    OrderBook {
        bids: vec![
            OrderBookOrders {
                price: 10.0,
                amount: 1.0,
                total: 41.0,
            },
            OrderBookOrders {
                price: 9.0,
                amount: 1.0,
                total: 42.0,
            },
        ],
        asks: vec![
            OrderBookOrders {
                price: 11.0,
                amount: 1.0,
                total: 43.0,
            },
            OrderBookOrders {
                price: 12.0,
                amount: 1.0,
                total: 44.0,
            },
        ],
    }
}
fn book(book: &OrderBook) -> Value {
    let rows = |rows: &[OrderBookOrders]| {
        rows.iter()
            .map(|row| json!([bits(row.price), bits(row.amount), bits(row.total)]))
            .collect::<Vec<_>>()
    };
    json!({"bids":rows(&book.bids),"asks":rows(&book.asks)})
}
fn cell(value: Option<&Value>) -> Value {
    match value {
        None => json!(["undefined"]),
        Some(Value::Null) => json!(["null"]),
        Some(Value::Bool(value)) => json!(["bool", value]),
        Some(value @ Value::Number(_)) => json!(["number", bits(js_value_as_f64(value).unwrap())]),
        Some(Value::String(value)) => json!(["string", value.encode_utf16().collect::<Vec<_>>()]),
        Some(Value::Array(values)) => json!([
            "array",
            values.iter().map(|v| cell(Some(v))).collect::<Vec<_>>()
        ]),
        Some(Value::Object(values)) => json!([
            "object",
            values
                .iter()
                .map(|(key, value)| json!([
                    key.encode_utf16().collect::<Vec<_>>(),
                    cell(Some(value))
                ]))
                .collect::<Vec<_>>()
        ]),
    }
}

pub fn snapshot(
    input: &Value,
    run: fn(&mut OrderBook, &[Vec<String>], &[Vec<String>], f64, &mut BinanceOrderBookSyncState),
) -> Value {
    let mut data = seed();
    let mut sync = state(&input["state"]);
    let bids: Vec<Vec<String>> = serde_json::from_value(input["bids"].clone()).unwrap();
    let asks: Vec<Vec<String>> = serde_json::from_value(input["asks"].clone()).unwrap();
    run(
        &mut data,
        &bids,
        &asks,
        number_from_string(input["id"].as_str().unwrap()),
        &mut sync,
    );
    json!({"book":book(&data),"state":observed_state(&sync)})
}

pub fn buffered(
    input: &Value,
    run: for<'a> fn(
        &'a [BinanceDepthUpdate],
        f64,
    ) -> Result<Vec<&'a BinanceDepthUpdate>, JsCoercionError>,
) -> Value {
    let updates: Vec<BinanceDepthUpdate> =
        serde_json::from_str(input["raw"].as_str().unwrap()).unwrap();
    match run(
        &updates,
        number_from_string(input["snapshot"].as_str().unwrap()),
    ) {
        Ok(rows) => {
            json!({"ok":true,"indices":rows.iter().map(|row| updates.iter().position(|original|std::ptr::eq(*row,original)).unwrap()).collect::<Vec<_>>(),
            "values":rows.iter().map(|row|cell(row.get("u"))).collect::<Vec<_>>() })
        }
        Err(error) => json!({"ok":false,"name":error.name}),
    }
}

pub fn apply(
    input: &Value,
    run: fn(
        &mut OrderBook,
        &BinanceDepthUpdate,
        &mut BinanceOrderBookSyncState,
        bool,
    ) -> Result<BinanceOrderBookApplyResult, JsCoercionError>,
) -> Value {
    let update: BinanceDepthUpdate = serde_json::from_str(input["raw"].as_str().unwrap()).unwrap();
    let mut data = seed();
    let mut sync = state(&input["state"]);
    let result = match run(
        &mut data,
        &update,
        &mut sync,
        input["previous"].as_bool().unwrap(),
    ) {
        Ok(value) => {
            json!({"value":match value {BinanceOrderBookApplyResult::Applied=>"applied",BinanceOrderBookApplyResult::Ignored=>"ignored",BinanceOrderBookApplyResult::Gap=>"gap"}})
        }
        Err(error) => json!({"error":error.name}),
    };
    json!({"result":result,"book":book(&data),"state":observed_state(&sync)})
}

pub fn index(input: &Value, read: fn(Option<&Value>, f64) -> Option<Value>) -> Value {
    let row: Value = serde_json::from_str(input["raw"].as_str().unwrap()).unwrap();
    let value = read(Some(&row), input["index"].as_f64().unwrap());
    if input["consumer"] == "parseFloat" {
        let result =
            safe_cex_num::js_string(value.as_ref()).map(|value| safe_cex_num::parse_float(&value));
        match result {
            Ok(value) => json!({"consumerBits":bits(value)}),
            Err(error) => json!({"error":error.name}),
        }
    } else {
        cell(value.as_ref())
    }
}
