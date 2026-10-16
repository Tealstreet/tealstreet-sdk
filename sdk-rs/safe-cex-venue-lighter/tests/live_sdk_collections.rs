use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_lighter::{
    sdk_common::{Account, OrderBook, PaginatedResponse},
    sdk_orders::BatchCancelOrdersParams,
};
use serde::{Deserialize, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn corpus() -> &'static Value {
    static CAPTURE: OnceLock<Value> = OnceLock::new();
    CAPTURE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/scripts/sdk-collections-oracle.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(
            capture["cases"].as_object().unwrap().len(),
            4,
            "SDK-CARDINALITY source record total"
        );
        capture
    })
}

fn check<T>(
    name: &str,
    field: &str,
    input: &Value,
    paths: Vec<(&str, T)>,
    native: impl Fn(&T) -> Vec<Value>,
) {
    let expected = input[field].as_array().unwrap();
    assert_eq!(
        expected.len(),
        2,
        "SDK-CARDINALITY {name}.{field} source length"
    );
    let path_count = if name == "PaginatedResponse" { 6 } else { 3 };
    assert_eq!(
        paths.len(),
        path_count,
        "SDK-CARDINALITY {name}.{field} entry paths"
    );
    let mut failures = Vec::new();
    let mut compared = 0;
    for (path, decoded) in paths {
        let actual = native(&decoded);
        if actual.len() != 2 {
            failures.push(format!("{path} native length {} != 2", actual.len()));
        }
        if &actual != expected {
            failures.push(format!("{path} native collection identity"));
        }
        for (index, value) in actual.iter().enumerate() {
            compared += 1;
            if expected.get(index) != Some(value) {
                failures.push(format!("{path} native item {index}"));
            }
        }
    }
    if compared != path_count * 2 {
        failures.push(format!(
            "native comparisons {compared} != {}",
            path_count * 2
        ));
    }
    assert!(
        failures.is_empty(),
        "SDK-CARDINALITY {name}.{field}: {failures:?}"
    );
}
fn bare<T: DeserializeOwned>(input: &Value) -> Vec<(&'static str, T)> {
    vec![
        (
            "bare raw",
            serde_json::from_str(&input.to_string()).expect("collection must decode successfully"),
        ),
        (
            "bare owned",
            T::deserialize(input.clone()).expect("collection must decode successfully"),
        ),
        (
            "bare borrowed",
            T::deserialize(input).expect("collection must decode successfully"),
        ),
    ]
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_balances() {
    let input = &corpus()["cases"]["account"];
    check("Account", "balances", input, bare::<Account>(input), |v| {
        v.balances.iter().map(|b| json!({"symbol":b.symbol,"tokenAddress":b.token_address,"available":b.available,"locked":b.locked,"total":b.total,"decimals":b.decimals})).collect()
    });
}
fn book(field: &str) {
    let mut input = corpus()["cases"]["book"].clone();
    input[if field == "bids" { "asks" } else { "bids" }] = json!([]);
    check("OrderBook", field, &input, bare::<OrderBook>(&input), |v| {
        let levels = if field == "bids" { &v.bids } else { &v.asks };
        levels
            .iter()
            .map(|l| json!({"price":l.price,"quantity":l.quantity}))
            .collect()
    });
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_bids() {
    book("bids");
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_asks() {
    book("asks");
}
fn cancel(field: &str) {
    let mut input = corpus()["cases"]["cancel"].clone();
    input[if field == "orderIds" {
        "clientOrderIds"
    } else {
        "orderIds"
    }] = json!([]);
    check(
        "BatchCancelOrdersParams",
        field,
        &input,
        bare::<BatchCancelOrdersParams>(&input),
        |v| {
            let ids = if field == "orderIds" {
                &v.order_ids
            } else {
                &v.client_order_ids
            };
            ids.as_ref()
                .expect("present ID array")
                .iter()
                .map(|s| json!(s))
                .collect()
        },
    );
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_order_ids() {
    cancel("orderIds");
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_client_order_ids() {
    cancel("clientOrderIds");
}
#[derive(Serialize, Deserialize)]
struct Item {
    value: f64,
}
impl JsNumberOutput for Item {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { value });
    }
}
#[test]
#[ignore = "requires Node and maintained TypeScript"]
fn sdk_collection_items() {
    let input = &corpus()["cases"]["page"];
    let mut paths = bare::<PaginatedResponse<Item>>(input);
    paths.extend([
        (
            "shared raw",
            js_from_str(&input.to_string()).expect("collection must decode successfully"),
        ),
        (
            "shared owned",
            js_from_value(input.clone()).expect("collection must decode successfully"),
        ),
        (
            "shared borrowed",
            js_from_value_ref(input).expect("collection must decode successfully"),
        ),
    ]);
    check("PaginatedResponse", "items", input, paths, |v| {
        v.items.iter().map(|i| json!({"value":i.value})).collect()
    });
}
