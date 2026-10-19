use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::sdk_order_api::{AccountOrdersResponse, ApiOrder};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::process::Command;
fn capture() -> &'static Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-order-api-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        {
            let value: Value = serde_json::from_slice(&output.stdout).unwrap();
            assert_eq!(
                value.get("contentHash").unwrap(),
                "e0117b9257f0a13eebe4814e37a12847dd425d1e64e31436309b8bb414881bdb",
                "ORDER-API live corpus digest"
            );
            value
        }
    })
}
fn paths<T: DeserializeOwned + Serialize + JsNumberOutput>(raw: &str) -> Vec<(&'static str, T)> {
    let value: Value = serde_json::from_str(raw).unwrap();
    vec![
        ("bare raw", serde_json::from_str(raw).unwrap()),
        ("bare owned", T::deserialize(value.clone()).unwrap()),
        ("bare borrowed", T::deserialize(&value).unwrap()),
        ("shared raw", js_from_str(raw).unwrap()),
        ("shared owned", js_from_value(value.clone()).unwrap()),
        ("shared borrowed", js_from_value_ref(&value).unwrap()),
    ]
}
fn identity(order: &ApiOrder, expected: &Value, label: &str) {
    for (field, native) in [
        ("orderId", &order.order_id),
        ("symbol", &order.symbol),
        ("price", &order.price),
        ("quantity", &order.quantity),
        ("filledQuantity", &order.filled_quantity),
        ("remainingQuantity", &order.remaining_quantity),
    ] {
        assert_eq!(
            native,
            expected.get(field).unwrap().as_str().unwrap(),
            "{label} ApiOrder.{field} native identity"
        );
    }
    for (field, native) in [
        ("marketId", order.market_id),
        ("accountIndex", order.account_index),
        ("createdAt", order.created_at),
        ("updatedAt", order.updated_at),
        ("status", order.status.0),
    ] {
        assert_eq!(
            native.to_bits(),
            safe_cex_num::numeric_ingress::js_value_as_f64(expected.get(field).unwrap())
                .unwrap()
                .to_bits(),
            "{label} ApiOrder.{field} native identity"
        );
    }
    for (field, native) in [
        ("isBuy", order.is_buy),
        ("postOnly", order.post_only),
        ("reduceOnly", order.reduce_only),
    ] {
        assert_eq!(
            native,
            expected.get(field).unwrap().as_bool().unwrap(),
            "{label} ApiOrder.{field} native identity"
        );
    }
    assert_eq!(
        std::any::type_name_of_val(&order.status),
        std::any::type_name::<safe_cex_venue_lighter::sdk_protocol_types::OrderStatus>(),
        "ApiOrder.status field type"
    );
}
fn pinned_orders() -> Vec<Value> {
    (0..8).map(|i|json!({"orderId":format!("order-{i}"),"marketId":11.25,"symbol":"SYMBOL-B","accountIndex":22.5,"price":"31.25","quantity":"42.5","filledQuantity":"53.75","remainingQuantity":"64.125","isBuy":i&1!=0,"postOnly":i&2!=0,"reduceOnly":i&4!=0,"createdAt":75.5,"updatedAt":86.75,"status":2})).collect()
}
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_native_identity() {
    let oracle = capture();
    let expected = pinned_orders();
    assert_eq!(
        oracle.get("orders").unwrap(),
        &json!(expected),
        "ORDER-API source discriminating content"
    );
    for (index, input) in expected.iter().enumerate() {
        for (path, native) in paths::<ApiOrder>(&input.to_string()) {
            identity(&native, input, &format!("case {index} {path}"));
        }
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_collection_identity() {
    let expected = pinned_orders();
    assert_eq!(
        capture().get("orders").unwrap(),
        &json!(expected),
        "ORDER-API source collection content"
    );
    for indices in [vec![], vec![3], vec![5, 1, 5, 0]] {
        let items: Vec<_> = indices.iter().map(|i| expected[*i].clone()).collect();
        let input = json!({"orders":items,"total":901.25,"nextCursor":" cursor-next "});
        for (path, native) in paths::<AccountOrdersResponse>(&input.to_string()) {
            native_length(native.orders.len(), indices.len(), path);
            let mut comparisons = 0;
            for (order, index) in native.orders.iter().zip(&indices) {
                identity(order, &expected[*index], path);
                comparisons += 1;
            }
            comparison_count(comparisons, indices.len(), path);
            assert_eq!(
                native
                    .orders
                    .iter()
                    .map(|o| o.order_id.as_str())
                    .collect::<Vec<_>>(),
                indices
                    .iter()
                    .map(|i| expected[*i]["orderId"].as_str().unwrap())
                    .collect::<Vec<_>>(),
                "ORDER-API orders native collection identity {path}"
            );
            assert_eq!(
                native.total.unwrap().to_bits(),
                901.25f64.to_bits(),
                "AccountOrdersResponse.total native identity"
            );
            assert_eq!(
                native.next_cursor.as_deref(),
                Some(" cursor-next "),
                "AccountOrdersResponse.nextCursor native identity"
            );
        }
    }
}
fn number(order: &ApiOrder, field: &str) -> f64 {
    match field {
        "marketId" => order.market_id,
        "accountIndex" => order.account_index,
        "createdAt" => order.created_at,
        "updatedAt" => order.updated_at,
        "status" => order.status.0,
        _ => panic!("unknown numeric field"),
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_numeric_bits() {
    let rows = capture().get("numeric").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 45, "ORDER-API numeric inventory");
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
    let mut comparisons = 0;
    for (index, row) in rows.iter().enumerate() {
        let field = [
            "marketId",
            "accountIndex",
            "createdAt",
            "updatedAt",
            "status",
        ][index / 9];
        assert_eq!(row.get("field").unwrap(), field);
        assert_eq!(row.get("raw").unwrap(), lexemes[index % 9]);
        let bits = u64::from_str_radix(row.get("bits").unwrap().as_str().unwrap(), 16).unwrap();
        let raw = row.get("encoded").unwrap().as_str().unwrap();
        let mut expected = pinned_orders()[0].clone();
        expected["orderId"] = json!("order-A");
        expected["isBuy"] = json!(true);
        expected["reduceOnly"] = json!(true);
        expected[field] = json!("__NUMBER__");
        let expected_raw = expected
            .to_string()
            .replace("\"__NUMBER__\"", lexemes[index % 9]);
        assert_eq!(
            serde_json::from_str::<Value>(raw).unwrap(),
            serde_json::from_str::<Value>(&expected_raw).unwrap(),
            "ORDER-API numeric input content"
        );
        let fixed_bits = [
            0x8000000000000000u64,
            0x8000000000000000,
            0x8000000000000000,
            0,
            1000f64.to_bits(),
            f64::INFINITY.to_bits(),
            f64::NEG_INFINITY.to_bits(),
            16777217.25f64.to_bits(),
            0x8000000000000000,
        ];
        assert_eq!(bits, fixed_bits[index % 9], "ORDER-API numeric bit content");
        for (path, order) in paths::<ApiOrder>(raw) {
            assert_eq!(
                number(&order, field).to_bits(),
                bits,
                "ApiOrder.{field} {path} bits"
            );
            comparisons += 1;
        }
        for (path, response) in paths::<AccountOrdersResponse>(&format!("{{\"orders\":[{raw}]}}")) {
            assert_eq!(
                response.orders.len(),
                1,
                "ORDER-API nested numeric native length"
            );
            assert_eq!(
                number(&response.orders[0], field).to_bits(),
                bits,
                "AccountOrdersResponse.orders.{field} {path} bits"
            );
            comparisons += 1;
        }
    }
    assert_eq!(comparisons, 540, "ORDER-API numeric comparison inventory");
    let totals = capture().get("totals").unwrap().as_array().unwrap();
    assert_eq!(totals.len(), 9);
    for (index, row) in totals.iter().enumerate() {
        let raw = lexemes[index];
        assert_eq!(row.get("raw").unwrap(), raw);
        let encoded = format!("{{\"orders\":[],\"total\":{raw}}}");
        assert_eq!(row.get("encoded").unwrap().as_str().unwrap(), encoded);
        let expected = u64::from_str_radix(row.get("bits").unwrap().as_str().unwrap(), 16).unwrap();
        for (_, native) in paths::<AccountOrdersResponse>(&encoded) {
            assert_eq!(
                native.total.unwrap().to_bits(),
                expected,
                "AccountOrdersResponse.total bits"
            );
        }
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_string_identity() {
    let rows = capture().get("strings").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 54, "ORDER-API strings inventory");
    for (index, row) in rows.iter().enumerate() {
        let units = unicode_units();
        let field = [
            "orderId",
            "symbol",
            "price",
            "quantity",
            "filledQuantity",
            "remainingQuantity",
        ][index / 9];
        assert_eq!(row.get("field").unwrap(), field);
        assert_eq!(row.get("units").unwrap(), &json!(units[index % 9]));
        let raw = row.get("encoded").unwrap().as_str().unwrap();
        if row.get("representable").unwrap().as_bool().unwrap() {
            let expected: Value = serde_json::from_str(raw).unwrap();
            assert_eq!(
                expected
                    .get(field)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .encode_utf16()
                    .collect::<Vec<_>>(),
                units[index % 9],
                "ORDER-API string encoded content"
            );
            for (path, native) in paths::<ApiOrder>(raw) {
                identity(&native, &expected, path);
            }
            for (path, native) in paths::<AccountOrdersResponse>(&format!("{{\"orders\":[{raw}]}}"))
            {
                assert_eq!(native.orders.len(), 1);
                identity(&native.orders[0], &expected, path);
            }
        } else {
            let mut expected = capture()["base"].clone();
            expected[field] = json!(String::from_utf16_lossy(&units[index % 9]));
            for native in [
                serde_json::from_str::<ApiOrder>(raw).unwrap(),
                js_from_str::<ApiOrder>(raw).unwrap(),
            ] {
                typed_utf16::assert_projection(raw, &native);
                identity(&native, &expected, "raw typed surrogate");
            }
            let outer = format!("{{\"orders\":[{raw}]}}");
            for native in [
                serde_json::from_str::<AccountOrdersResponse>(&outer).unwrap(),
                js_from_str::<AccountOrdersResponse>(&outer).unwrap(),
            ] {
                assert_eq!(native.orders.len(), 1);
                typed_utf16::assert_projection(&outer, &native);
                identity(&native.orders[0], &expected, "raw typed nested surrogate");
            }
        }
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_optional_and_shape() {
    capture();
    for (_, value) in paths::<AccountOrdersResponse>("{\"orders\":[]}") {
        assert!(value.total.is_none());
        assert!(value.next_cursor.is_none());
    }
    for raw in [
        "{\"orders\":[],\"total\":null}",
        "{\"orders\":[],\"nextCursor\":null}",
        "{}",
        "[]",
        "null",
    ] {
        assert!(
            serde_json::from_str::<AccountOrdersResponse>(raw).is_err(),
            "AccountOrdersResponse shape {raw}"
        );
        assert!(js_from_str::<AccountOrdersResponse>(raw).is_err());
    }
    let base = capture().get("base").unwrap();
    for field in base.as_object().unwrap().keys() {
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            serde_json::from_value::<ApiOrder>(missing).is_err(),
            "ApiOrder required {field}"
        );
    }
}
fn depth_paths<T: DeserializeOwned + Serialize + JsNumberOutput>(
    raw: &str,
    input: &Value,
) -> Vec<(&'static str, Result<T, String>)> {
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
#[test]
#[ignore = "actual source Node oracle"]
fn order_api_unknown_boundaries() {
    let rows = capture().get("depth").unwrap().as_array().unwrap();
    assert_eq!(rows.len(), 15, "ORDER-API depth inventory");
    for (index, row) in rows.iter().enumerate() {
        let record = if index < 5 {
            "ApiOrder"
        } else {
            "AccountOrdersResponse"
        };
        let nested = index >= 10;
        let count = [124, 125, 126, 127, 140][index % 5];
        assert_eq!(row.get("record").unwrap(), record);
        assert_eq!(row.get("nested").unwrap(), nested);
        assert_eq!(row.get("count").unwrap(), count);
        let raw = row.get("encoded").unwrap().as_str().unwrap();
        let mut input = if record == "ApiOrder" {
            capture()["base"].clone()
        } else {
            json!({"orders":[capture()["base"].clone()]})
        };
        let mut extra = json!(17);
        for _ in 0..count {
            extra = json!([extra]);
        }
        if nested {
            input["orders"][0]["extra"] = extra;
        } else {
            input["extra"] = extra;
        }
        let accepted = count <= if nested { 124 } else { 126 };
        let results = if record == "ApiOrder" {
            depth_paths::<ApiOrder>(raw, &input)
                .into_iter()
                .map(|(p, r)| (p, r.map(|_| ())))
                .collect::<Vec<_>>()
        } else {
            depth_paths::<AccountOrdersResponse>(raw, &input)
                .into_iter()
                .map(|(p, r)| (p, r.map(|_| ())))
                .collect()
        };
        for (path, result) in results {
            assert_eq!(
                result.is_ok(),
                accepted,
                "{record} {path} unknown depth {count} nested={nested}: {result:?}"
            );
            if let Err(error) = result {
                assert!(error.contains("recursion") || error.contains("depth"));
            }
        }
    }
    for record in ["ApiOrder", "AccountOrdersResponse"] {
        for escaped in ["\\ud800", "\\udfff", "x\\ud800y"] {
            let raw = if record == "ApiOrder" {
                let base = capture()["base"].to_string();
                format!("{},\"extra\":\"{escaped}\"}}", &base[..base.len() - 1])
            } else {
                format!("{{\"orders\":[],\"extra\":\"{escaped}\"}}")
            };
            typed_utf16::retained(&raw);
            if record == "ApiOrder" {
                for order in [
                    serde_json::from_str::<ApiOrder>(&raw).unwrap(),
                    js_from_str::<ApiOrder>(&raw).unwrap(),
                ] {
                    identity(&order, &capture()["base"], "unknown surrogate ignored");
                }
            } else {
                for response in [
                    serde_json::from_str::<AccountOrdersResponse>(&raw).unwrap(),
                    js_from_str::<AccountOrdersResponse>(&raw).unwrap(),
                ] {
                    assert!(
                        response.orders.is_empty(),
                        "unknown surrogate does not change orders"
                    );
                }
            }
        }
    }
}
#[test]
#[ignore = "actual source Node mutation controls"]
fn order_api_source_controls() {
    for (mutation, test, name) in [
        (
            "surrogate-content",
            "order_api_native_identity",
            "ORDER-API live corpus content",
        ),
        (
            "missing-import",
            "order_api_native_identity",
            "order API OrderStatus import inventory",
        ),
        (
            "missing-enum",
            "order_api_native_identity",
            "order API selected program diagnostics must be empty",
        ),
        (
            "heritage",
            "order_api_native_identity",
            "ApiOrder unsupported interface heritage",
        ),
        (
            "response-heritage",
            "order_api_native_identity",
            "AccountOrdersResponse unsupported interface heritage",
        ),
        (
            "status-type",
            "order_api_native_identity",
            "ApiOrder source schema",
        ),
        (
            "content",
            "order_api_native_identity",
            "ORDER-API live corpus content",
        ),
        (
            "reverse",
            "order_api_native_identity",
            "ORDER-API live corpus content",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args(["--exact", test, "--include-ignored", "--nocapture"])
            .env("LIGHTER_ORDER_API_MUTATION", mutation)
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
fn unicode_units() -> Vec<Vec<u16>> {
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
fn order_api_cursor_and_extra_units() {
    let corpus = capture();
    let units = unicode_units();
    let cursors = corpus.get("cursors").unwrap().as_array().unwrap();
    assert_eq!(cursors.len(), 9);
    for (index, row) in cursors.iter().enumerate() {
        assert_eq!(row.get("units").unwrap(), &json!(units[index]));
        let raw = row.get("encoded").unwrap().as_str().unwrap();
        let expected = String::from_utf16(&units[index]);
        assert_eq!(row.get("representable").unwrap(), expected.is_ok());
        if let Ok(text) = expected {
            for (path, native) in paths::<AccountOrdersResponse>(raw) {
                assert_eq!(
                    native.next_cursor.as_deref(),
                    Some(text.as_str()),
                    "AccountOrdersResponse.nextCursor {path} native units"
                );
            }
        } else {
            for native in [
                serde_json::from_str::<AccountOrdersResponse>(raw).unwrap(),
                js_from_str::<AccountOrdersResponse>(raw).unwrap(),
            ] {
                typed_utf16::assert_projection(raw, &native);
                assert_eq!(
                    native.next_cursor.as_deref(),
                    Some(String::from_utf16_lossy(&units[index]).as_str())
                );
            }
        }
    }
    let extras = corpus.get("extras").unwrap().as_array().unwrap();
    assert_eq!(extras.len(), 27);
    for (index, row) in extras.iter().enumerate() {
        let record = if index < 9 {
            "ApiOrder"
        } else {
            "AccountOrdersResponse"
        };
        let nested = index >= 18;
        let expected_units = &units[index % 9];
        assert_eq!(row.get("record").unwrap(), record);
        assert_eq!(row.get("nested").unwrap(), nested);
        assert_eq!(row.get("units").unwrap(), &json!(expected_units));
        assert_eq!(row.get("type").unwrap(), "string");
        assert_eq!(row.get("length").unwrap(), expected_units.len());
        let raw = row.get("encoded").unwrap().as_str().unwrap();
        let representable = String::from_utf16(expected_units).is_ok();
        assert_eq!(row.get("representable").unwrap(), representable);
        if representable {
            if record == "ApiOrder" {
                for (path, native) in paths::<ApiOrder>(raw) {
                    identity(&native, &corpus["base"], path);
                }
            } else {
                for (path, native) in paths::<AccountOrdersResponse>(raw) {
                    assert_eq!(native.orders.len(), 1);
                    identity(&native.orders[0], &corpus["base"], path);
                }
            }
        } else if record == "ApiOrder" {
            typed_utf16::retained(raw);
            for native in [
                serde_json::from_str::<ApiOrder>(raw).unwrap(),
                js_from_str::<ApiOrder>(raw).unwrap(),
            ] {
                identity(&native, &corpus["base"], "raw unknown surrogate");
            }
        } else {
            typed_utf16::retained(raw);
            for native in [
                serde_json::from_str::<AccountOrdersResponse>(raw).unwrap(),
                js_from_str::<AccountOrdersResponse>(raw).unwrap(),
            ] {
                assert_eq!(native.orders.len(), 1);
                identity(
                    &native.orders[0],
                    &corpus["base"],
                    "raw nested unknown surrogate",
                );
            }
        }
    }
}

fn native_length(actual: usize, expected: usize, label: &str) {
    assert_eq!(actual, expected, "ORDER-API orders native length {label}");
}
fn comparison_count(actual: usize, expected: usize, label: &str) {
    assert_eq!(
        actual, expected,
        "ORDER-API orders comparison count {label}"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
