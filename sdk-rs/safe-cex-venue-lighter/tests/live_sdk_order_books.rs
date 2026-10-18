use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::sdk_common::OrderBookLevel;
use safe_cex_venue_lighter::sdk_order_api::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;

fn capture() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-order-books-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let value: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            value["contentHash"],
            "07bd223cebfa73b9071ff9049ab45237a1818598f1294a335502c7b2f8c725e2",
            "BOOKS live corpus digest"
        );
        value
    })
}
fn n(value: f64) -> Value {
    json!({"bits":format!("{:016x}", value.to_bits())})
}
fn level(v: &OrderBookLevel) -> Value {
    json!({"price":v.price,"quantity":v.quantity})
}
fn book(v: &ApiOrderBook) -> Value {
    json!({"marketId":n(v.market_id),"symbol":v.symbol,"bestBid":v.best_bid,"bestAsk":v.best_ask,
        "bids":v.bids.iter().map(level).collect::<Vec<_>>(),"asks":v.asks.iter().map(level).collect::<Vec<_>>(),"timestamp":n(v.timestamp)})
}
fn order_level(v: &ApiOrderBookOrderLevel) -> Value {
    json!({"price":v.price,"quantity":v.quantity,"orderCount":n(v.order_count)})
}
fn details(v: &ApiOrderBookDetails) -> Value {
    json!({"marketId":n(v.market_id),"symbol":v.symbol,"totalBidVolume":v.total_bid_volume,"totalAskVolume":v.total_ask_volume,
        "bidOrderCount":n(v.bid_order_count),"askOrderCount":n(v.ask_order_count),"midPrice":v.mid_price,"spread":v.spread,
        "spreadPercentage":v.spread_percentage,"lastUpdate":n(v.last_update)})
}
// Projection reads native fields individually. It never calls their Serialize implementation.
trait Native: DeserializeOwned + Serialize + JsNumberOutput {
    fn project(&self) -> Value;
}
impl Native for OrderBooksResponse {
    fn project(&self) -> Value {
        json!({"orderBooks":self.order_books.iter().map(book).collect::<Vec<_>>()})
    }
}
impl Native for OrderBookOrdersResponse {
    fn project(&self) -> Value {
        json!({"marketId":n(self.market_id),"timestamp":n(self.timestamp),
        "bids":self.bids.iter().map(order_level).collect::<Vec<_>>(),"asks":self.asks.iter().map(order_level).collect::<Vec<_>>()})
    }
}
impl Native for OrderBookDetailsResponse {
    fn project(&self) -> Value {
        json!({"details":self.details.iter().map(details).collect::<Vec<_>>()})
    }
}
fn paths<T: Native>(raw: &str, input: &Value) -> Vec<(&'static str, Result<T, String>)> {
    vec![
        (
            "bare raw",
            serde_json::from_str(raw).map_err(|e| e.to_string()),
        ),
        (
            "bare owned",
            T::deserialize(input.clone()).map_err(|e| e.to_string()),
        ),
        (
            "bare borrowed",
            T::deserialize(input).map_err(|e| e.to_string()),
        ),
        ("shared raw", js_from_str(raw).map_err(|e| e.to_string())),
        (
            "shared owned",
            js_from_value(input.clone()).map_err(|e| e.to_string()),
        ),
        (
            "shared borrowed",
            js_from_value_ref(input).map_err(|e| e.to_string()),
        ),
    ]
}
fn project_paths<T: Native>(
    raw: &str,
    input: &Value,
) -> Vec<(&'static str, Result<Value, String>)> {
    paths::<T>(raw, input)
        .into_iter()
        .map(|(path, v)| (path, v.map(|v| v.project())))
        .collect()
}
fn decode(record: &str, raw: &str, input: &Value) -> Vec<(&'static str, Result<Value, String>)> {
    match record {
        "OrderBooksResponse" => project_paths::<OrderBooksResponse>(raw, input),
        "OrderBookOrdersResponse" => project_paths::<OrderBookOrdersResponse>(raw, input),
        "OrderBookDetailsResponse" => project_paths::<OrderBookDetailsResponse>(raw, input),
        _ => panic!("unknown record {record}"),
    }
}
fn expected(value: &Value) -> Value {
    match value {
        Value::Number(_) => n(safe_cex_num::numeric_ingress::js_value_as_f64(value).unwrap()),
        Value::Array(items) => Value::Array(items.iter().map(expected).collect()),
        Value::Object(fields) => Value::Object(
            fields
                .iter()
                .map(|(k, v)| (k.clone(), expected(v)))
                .collect(),
        ),
        _ => value.clone(),
    }
}
fn at<'a>(mut value: &'a Value, keys: &[Value]) -> &'a Value {
    for key in keys {
        value = if let Some(key) = key.as_str() {
            &value[key]
        } else {
            &value[key.as_u64().unwrap() as usize]
        };
    }
    value
}
fn at_mut<'a>(mut value: &'a mut Value, keys: &[Value]) -> &'a mut Value {
    for key in keys {
        value = if let Some(key) = key.as_str() {
            &mut value[key]
        } else {
            &mut value[key.as_u64().unwrap() as usize]
        };
    }
    value
}
fn make_level(i: usize, side: &str) -> Value {
    json!({"price":format!("{side}-price-{i}"),"quantity":format!("{side}-quantity-{i}")})
}
fn make_book(i: usize) -> Value {
    json!({"marketId":11.25+i as f64,"symbol":format!("symbol-{i}"),"bestBid":format!("best-bid-{i}"),"bestAsk":format!("best-ask-{i}"),"bids":[make_level(1,"bid"),make_level(3,"bid"),make_level(1,"bid"),make_level(5,"bid")],"asks":[make_level(2,"ask"),make_level(4,"ask")],"timestamp":81.75+i as f64})
}
fn make_details(i: usize) -> Value {
    json!({"marketId":21.25+i as f64,"symbol":format!("details-{i}"),"totalBidVolume":format!("bid-volume-{i}"),"totalAskVolume":format!("ask-volume-{i}"),"bidOrderCount":31.5+i as f64,"askOrderCount":42.75+i as f64,"midPrice":format!("mid-{i}"),"spread":format!("spread-{i}"),"spreadPercentage":format!("percent-{i}"),"lastUpdate":93.125+i as f64})
}
fn bases() -> Value {
    json!({
        "OrderBooksResponse":{"orderBooks":[make_book(2),make_book(0),make_book(2),make_book(1)]},
        "OrderBookOrdersResponse":{"marketId":73.25,"bids":[{"price":"bid-price-1","quantity":"bid-quantity-1","orderCount":14.5},{"price":"bid-price-2","quantity":"bid-quantity-2","orderCount":25.75}],"asks":[{"price":"ask-price-3","quantity":"ask-quantity-3","orderCount":36.125}],"timestamp":84.5},
        "OrderBookDetailsResponse":{"details":[make_details(3),make_details(1),make_details(3),make_details(0)]}
    })
}
const RECORDS: [&str; 3] = [
    "OrderBooksResponse",
    "OrderBookOrdersResponse",
    "OrderBookDetailsResponse",
];
#[test]
#[ignore = "actual source Node oracle"]
fn books_native_identity() {
    assert_eq!(
        capture()["bases"],
        bases(),
        "BOOKS independent fixture content"
    );
    for record in RECORDS {
        let input = bases()[record].clone();
        for (path, v) in decode(record, &input.to_string(), &input) {
            assert_eq!(
                v.unwrap(),
                expected(&input),
                "BOOKS {record} {path} native field identity"
            );
        }
    }
    // All native assertions precede wire shape checks.
    let input = bases();
    for (record, wire) in [
        (
            RECORDS[0],
            serde_json::to_value(
                serde_json::from_value::<OrderBooksResponse>(input[RECORDS[0]].clone()).unwrap(),
            )
            .unwrap(),
        ),
        (
            RECORDS[1],
            serde_json::to_value(
                serde_json::from_value::<OrderBookOrdersResponse>(input[RECORDS[1]].clone())
                    .unwrap(),
            )
            .unwrap(),
        ),
        (
            RECORDS[2],
            serde_json::to_value(
                serde_json::from_value::<OrderBookDetailsResponse>(input[RECORDS[2]].clone())
                    .unwrap(),
            )
            .unwrap(),
        ),
    ] {
        assert_eq!(wire, input[record], "BOOKS wire shape {record}");
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn books_numeric_bits() {
    let rows = capture()["numeric"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        90,
        "BOOKS numeric inventory: ten leaves times nine lexemes"
    );
    let lexemes = [
        "-0",
        "-0.0",
        "-0e0",
        "0",
        "1e3",
        "1e400",
        "-1e400",
        "16777217.25",
        "-1e-400",
    ];
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(row["raw"], lexemes[index % 9]);
        let raw = row["encoded"].as_str().unwrap();
        let input: Value = serde_json::from_str(raw).unwrap();
        let record = row["record"].as_str().unwrap();
        let keys = row["keys"].as_array().unwrap();
        for (path, v) in decode(record, raw, &input) {
            assert_eq!(
                at(&v.unwrap(), keys)["bits"],
                row["bits"],
                "BOOKS {record} {path} numeric bits {keys:?}"
            );
        }
    }
}
fn axes() -> Vec<(&'static str, Vec<Value>)> {
    vec![
        (RECORDS[0], vec![json!("orderBooks")]),
        (
            RECORDS[0],
            vec![json!("orderBooks"), json!(0), json!("bids")],
        ),
        (
            RECORDS[0],
            vec![json!("orderBooks"), json!(0), json!("asks")],
        ),
        (RECORDS[1], vec![json!("bids")]),
        (RECORDS[1], vec![json!("asks")]),
        (RECORDS[2], vec![json!("details")]),
    ]
}
#[test]
#[ignore = "actual source Node oracle"]
fn books_collection_identity() {
    let rows = capture()["collections"].as_array().unwrap();
    assert_eq!(rows.len(), 18, "BOOKS collection inventory");
    for ((record, keys), group) in axes().into_iter().zip(rows.as_chunks::<3>().0.iter()) {
        for ((mode, count), row) in [("empty", 0), ("single", 1), ("duplicate", 4)]
            .into_iter()
            .zip(group)
        {
            assert_eq!(row["record"], record);
            assert_eq!(row["keys"], json!(keys));
            assert_eq!(row["mode"], mode);
            assert_eq!(row["count"], count);
            let mut input = bases()[record].clone();
            let first = at(&input, &keys)[0].clone();
            *at_mut(&mut input, &keys) = json!(vec![first; count]);
            let raw = row["encoded"].as_str().unwrap();
            assert_eq!(
                serde_json::from_str::<Value>(raw).unwrap(),
                input,
                "BOOKS collection encoded content"
            );
            for (path, v) in decode(record, raw, &input) {
                let native = v.unwrap();
                assert_eq!(
                    at(&native, &keys).as_array().unwrap().len(),
                    count,
                    "BOOKS collection native length {record} {path}"
                );
                assert_eq!(
                    native,
                    expected(&input),
                    "BOOKS collection native identity {record} {path}"
                );
            }
        }
    }
}
fn units() -> Vec<Vec<u16>> {
    vec![
        vec![],
        vec![32, 120, 32],
        vec![233],
        vec![0xd83d, 0xdc69],
        vec![0xfeff],
        vec![0x85],
        vec![0xd800],
        vec![0xdfff],
        vec![120, 0xd800, 121],
    ]
}
#[test]
#[ignore = "actual source Node oracle"]
fn books_unicode_and_extras() {
    for (kind, count) in [("strings", 153), ("extras", 81)] {
        let rows = capture()[kind].as_array().unwrap();
        assert_eq!(rows.len(), count, "BOOKS {kind} inventory");
        for (index, row) in rows.iter().enumerate() {
            let expected_units = &units()[index % 9];
            let representable = String::from_utf16(expected_units);
            assert_eq!(row["units"], json!(expected_units));
            assert_eq!(row["length"], expected_units.len());
            assert_eq!(row["type"], "string");
            assert_eq!(row["representable"], representable.is_ok());
            let record = row["record"].as_str().unwrap();
            let raw = row["encoded"].as_str().unwrap();
            let keys = row["keys"].as_array().unwrap();
            if let Ok(text) = representable {
                let input: Value = serde_json::from_str(raw).unwrap();
                for (path, v) in decode(record, raw, &input) {
                    let native = v.unwrap();
                    if kind == "strings" {
                        assert_eq!(
                            at(&native, keys).as_str(),
                            Some(text.as_str()),
                            "BOOKS {record} {path} native UTF16 identity"
                        );
                    } else {
                        assert_eq!(
                            native,
                            expected(&bases()[record]),
                            "BOOKS {record} {path} unknown extra identity"
                        );
                    }
                }
            } else {
                let text = String::from_utf16_lossy(expected_units);
                match record {
                    "OrderBooksResponse" => {
                        raw_unicode::<OrderBooksResponse>(raw, kind, keys, &text, record)
                    }
                    "OrderBookOrdersResponse" => {
                        raw_unicode::<OrderBookOrdersResponse>(raw, kind, keys, &text, record)
                    }
                    "OrderBookDetailsResponse" => {
                        raw_unicode::<OrderBookDetailsResponse>(raw, kind, keys, &text, record)
                    }
                    _ => panic!("unknown record"),
                }
            }
        }
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn books_unknown_depth() {
    let rows = capture()["depths"].as_array().unwrap();
    assert_eq!(
        rows.len(),
        63,
        "BOOKS depth inventory: nine locations times seven counts"
    );
    for (index, row) in rows.iter().enumerate() {
        let count = [122, 123, 124, 125, 126, 127, 140][index % 7];
        assert_eq!(row["count"], count);
        let record = row["record"].as_str().unwrap();
        let keys = row["keys"].as_array().unwrap();
        let mut input = bases()[record].clone();
        let mut extra = json!(17);
        for _ in 0..count {
            extra = json!([extra]);
        }
        at_mut(&mut input, keys)["extra"] = extra;
        let accepted = count <= 126 - keys.len();
        let raw = row["encoded"].as_str().unwrap();
        for (path, result) in decode(record, raw, &input) {
            assert_eq!(
                result.is_ok(),
                accepted,
                "BOOKS {record} {path} unknown depth {count} {keys:?}: {result:?}"
            );
            match result {
                Ok(native) => assert_eq!(native, expected(&bases()[record])),
                Err(error) => assert!(error.contains("depth") || error.contains("recursion")),
            }
        }
    }
}
fn required_fields(value: &Value, keys: Vec<Value>) -> Vec<Vec<Value>> {
    match value {
        Value::Object(fields) => fields
            .iter()
            .flat_map(|(key, value)| {
                let mut keys = keys.clone();
                keys.push(json!(key));
                let mut out = vec![keys.clone()];
                out.extend(required_fields(value, keys));
                out
            })
            .collect(),
        Value::Array(items) if !items.is_empty() => {
            let mut keys = keys;
            keys.push(json!(0));
            required_fields(&items[0], keys)
        }
        _ => vec![],
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn books_required_shapes() {
    capture();
    for record in RECORDS {
        let base = bases()[record].clone();
        for keys in required_fields(&base, vec![]) {
            for null in [false, true] {
                let mut input = base.clone();
                let parent = at_mut(&mut input, &keys[..keys.len() - 1]);
                let name = keys.last().unwrap().as_str().unwrap();
                if null {
                    parent[name] = Value::Null;
                } else {
                    parent.as_object_mut().unwrap().remove(name);
                }
                for (path, result) in decode(record, &input.to_string(), &input) {
                    assert!(
                        result.is_err(),
                        "BOOKS required field {record} {keys:?} {path} null={null}"
                    );
                }
            }
        }
        for input in [json!([]), Value::Null, json!(17), json!("bad")] {
            for (path, result) in decode(record, &input.to_string(), &input) {
                assert!(result.is_err(), "BOOKS map-only {record} {path}");
            }
        }
    }
}
#[test]
#[ignore = "actual source Node mutation controls"]
fn books_source_controls() {
    for (mutation, name) in [
        ("missing-import", "BOOKS actual common import inventory"),
        ("missing-level", "BOOKS selected program diagnostics"),
        ("level-type", "BOOKS imported level schema"),
        ("level-optional", "BOOKS imported level schema"),
        ("level-method", "BOOKS imported level supported property"),
        (
            "heritage-OrderBooksResponse",
            "OrderBooksResponse unsupported heritage",
        ),
        (
            "heritage-OrderBookOrdersResponse",
            "OrderBookOrdersResponse unsupported heritage",
        ),
        (
            "heritage-OrderBookDetailsResponse",
            "OrderBookDetailsResponse unsupported heritage",
        ),
        ("content", "BOOKS live corpus digest"),
        ("reverse", "BOOKS live corpus digest"),
        ("surrogate-content", "BOOKS live corpus digest"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "books_native_identity",
                "--include-ignored",
                "--nocapture",
            ])
            .env("LIGHTER_ORDER_BOOKS_MUTATION", mutation)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{mutation}: {text}");
        assert!(
            text.contains(name),
            "{mutation} named failure {name}: {text}"
        );
    }
}

fn raw_unicode<T: Native>(raw: &str, kind: &str, keys: &[Value], text: &str, record: &str) {
    typed_utf16::retained(raw);
    for native in [
        serde_json::from_str::<T>(raw).unwrap(),
        js_from_str::<T>(raw).unwrap(),
    ] {
        let observed = native.project();
        if kind == "strings" {
            typed_utf16::assert_projection(raw, &native);
            assert_eq!(
                at(&observed, keys).as_str(),
                Some(text),
                "BOOKS typed String native replacement"
            );
        } else {
            assert_eq!(
                observed,
                expected(&bases()[record]),
                "BOOKS unknown surrogate preserves native fields"
            );
        }
    }
}
#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
