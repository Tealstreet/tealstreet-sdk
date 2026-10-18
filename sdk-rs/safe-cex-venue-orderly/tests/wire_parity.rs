use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_venue_orderly::wire::*;
use serde::{Serialize, de::DeserializeOwned};
use serde_json::{Map, Value, json};
use std::process::Command;

type View = Map<String, Value>;
trait Observe {
    fn observe(&self, path: &str, view: &mut View);
}
impl Observe for f64 {
    fn observe(&self, path: &str, view: &mut View) {
        let bits = if self.is_nan() {
            0x7ff8000000000000
        } else {
            self.to_bits()
        };
        view.insert(path.into(), json!(["number", format!("{bits:016x}")]));
    }
}
impl Observe for String {
    fn observe(&self, path: &str, view: &mut View) {
        view.insert(path.into(), json!(["string", self]));
    }
}
impl Observe for bool {
    fn observe(&self, path: &str, view: &mut View) {
        view.insert(path.into(), json!(["boolean", self]));
    }
}
impl<T: Observe> Observe for Option<T> {
    fn observe(&self, path: &str, view: &mut View) {
        if let Some(value) = self {
            value.observe(path, view);
        }
    }
}
impl<T: Observe> Observe for Vec<T> {
    fn observe(&self, path: &str, view: &mut View) {
        view.insert(path.into(), json!(["array", self.len()]));
        for (i, value) in self.iter().enumerate() {
            value.observe(&format!("{path}/{i}"), view);
        }
    }
}
impl Observe for AccountMode {
    fn observe(&self, path: &str, view: &mut View) {
        let Self::Futures = self;
        "FUTURES".to_string().observe(path, view);
    }
}
impl Observe for EditOrderType {
    fn observe(&self, path: &str, view: &mut View) {
        let Self::Limit = self;
        "LIMIT".to_string().observe(path, view);
    }
}
impl Observe for EditOrderSide {
    fn observe(&self, path: &str, view: &mut View) {
        match self {
            Self::Buy => "BUY",
            Self::Sell => "SELL",
        }
        .to_string()
        .observe(path, view);
    }
}
// Native field reads, independent of Serialize/Deserialize names. A symmetric
// codec-name swap still changes this view and fails the named field assertion.
macro_rules! record {
    ($ty:ty, $( $field:ident => $wire:literal ),+ $(,)?) => {
        impl Observe for $ty {
            fn observe(&self, path: &str, view: &mut View) {
                view.insert(path.into(), json!(["object"]));
                $(self.$field.observe(&format!("{path}/{}", $wire), view);)+
            }
        }
    };
}
record!(AccountResponse, account_id => "account_id", email => "email", account_mode => "account_mode",
    max_leverage => "max_leverage", taker_fee_rate => "taker_fee_rate", maker_fee_rate => "maker_fee_rate",
    futures_taker_fee_rate => "futures_taker_fee_rate", futures_maker_fee_rate => "futures_maker_fee_rate",
    maintenance_cancel_orders => "maintenance_cancel_orders");
record!(BalanceHolding, updated_time => "updated_time", token => "token", holding => "holding",
    frozen => "frozen", pending_short => "pending_short", available_balance => "availableBalance");
record!(BalanceResponse, holding => "holding");
record!(PlaceOrderData, order_id => "order_id", order_type => "order_type", order_price => "order_price",
    order_quantity => "order_quantity", order_amount => "order_amount", client_order_id => "client_order_id");
record!(PlaceOrderResponse, success => "success", timestamp => "timestamp", data => "data");
record!(EditOrderRequest, order_id => "order_id", symbol => "symbol", order_type => "order_type",
    side => "side", order_price => "order_price", order_quantity => "order_quantity");
record!(EditOrderData, status => "status");
record!(EditOrderResponse, success => "success", timestamp => "timestamp", code => "code",
    message => "message", data => "data");

fn field_inventory(name: &str) -> &'static [&'static str] {
    match name {
        "ACCOUNT_RESPONSE" => &[
            "",
            "/account_id",
            "/account_mode",
            "/email",
            "/futures_maker_fee_rate",
            "/futures_taker_fee_rate",
            "/maintenance_cancel_orders",
            "/maker_fee_rate",
            "/max_leverage",
            "/taker_fee_rate",
        ],
        "BALANCE_RESPONSE" => &[
            "",
            "/holding",
            "/holding/0",
            "/holding/0/availableBalance",
            "/holding/0/frozen",
            "/holding/0/holding",
            "/holding/0/pending_short",
            "/holding/0/token",
            "/holding/0/updated_time",
            "/holding/1",
            "/holding/1/availableBalance",
            "/holding/1/frozen",
            "/holding/1/holding",
            "/holding/1/pending_short",
            "/holding/1/token",
            "/holding/1/updated_time",
        ],
        "PLACE_ORDER_RESPONSE" => &[
            "",
            "/data",
            "/data/client_order_id",
            "/data/order_amount",
            "/data/order_id",
            "/data/order_price",
            "/data/order_quantity",
            "/data/order_type",
            "/success",
            "/timestamp",
        ],
        "EDIT_ORDER_REQUEST" => &[
            "",
            "/order_id",
            "/order_price",
            "/order_quantity",
            "/order_type",
            "/side",
            "/symbol",
        ],
        "EDIT_ORDER_RESPONSE" => &[
            "",
            "/code",
            "/data",
            "/data/status",
            "/message",
            "/success",
            "/timestamp",
        ],
        _ => panic!("unknown wire inventory {name}"),
    }
}

// Fixed counts for these fixture modes; neither expected nor native observations
// determine how many field comparisons a case must execute.
fn field_count(name: &str, mode: &str) -> usize {
    match (name, mode) {
        ("ACCOUNT_RESPONSE" | "PLACE_ORDER_RESPONSE", _) => 10,
        ("BALANCE_RESPONSE", _) => 16,
        ("EDIT_ORDER_REQUEST", "omit") => 5,
        ("EDIT_ORDER_REQUEST", "omit-field/order_price" | "omit-field/order_quantity") => 6,
        ("EDIT_ORDER_REQUEST", _) => 7,
        ("EDIT_ORDER_RESPONSE", "omit") => 2,
        ("EDIT_ORDER_RESPONSE", "omit-field/data") => 5,
        (
            "EDIT_ORDER_RESPONSE",
            "omit-field/timestamp"
            | "omit-field/code"
            | "omit-field/message"
            | "omit-field/data/status",
        ) => 6,
        ("EDIT_ORDER_RESPONSE", _) => 7,
        _ => panic!("unknown comparison count {name}/{mode}"),
    }
}

fn compare<T: DeserializeOwned + Serialize + JsNumberOutput + Observe>(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let mode = row["mode"].as_str().unwrap();
    let raw = row["raw"].as_str().unwrap();
    let input: Value = serde_json::from_str(raw).unwrap();
    let decoded = [
        js_from_str::<T>(raw),
        js_from_value::<T>(input.clone()),
        js_from_value_ref::<T>(&input),
    ];
    let mut compared_ingresses = 0;
    for (ingress, value) in decoded.into_iter().enumerate() {
        let value =
            value.unwrap_or_else(|error| panic!("wire/{name}/{mode}/ingress{ingress}: {error}"));
        let mut view = View::new();
        value.observe("", &mut view);
        let expected = row["observed"].as_object().unwrap();
        assert_eq!(
            view.keys().collect::<Vec<_>>().len(),
            expected.len(),
            "wire/{name}/{mode}/field census"
        );
        let paths: Vec<_> = field_inventory(name)
            .iter()
            .copied()
            .filter(|path| input.pointer(path).is_some())
            .collect();
        assert_eq!(
            expected.len(),
            field_count(name, mode),
            "ORDERLY-CARDINALITY wire/source field inventory/{name}/{mode}"
        );
        let mut compared_fields = 0;
        for path in &paths {
            let expected = expected.get(*path).expect("source inventory field present");
            if expected[0] == "array" {
                let source_length = input.pointer(path).unwrap().as_array().unwrap().len();
                assert_eq!(
                    expected[1].as_u64().unwrap() as usize,
                    source_length,
                    "ORDERLY-CARDINALITY wire/source array/{name}/{mode}/{path}"
                );
                assert_eq!(
                    view.get(*path).unwrap()[1].as_u64().unwrap() as usize,
                    source_length,
                    "ORDERLY-CARDINALITY wire/native array/{name}/{mode}/{path}"
                );
            }
            assert_eq!(
                view.get(*path),
                Some(expected),
                "wire/{name}/{mode}/native{path}/ingress{ingress}"
            );
            compared_fields += 1;
        }
        assert_eq!(
            compared_fields,
            field_count(name, mode),
            "ORDERLY-COMPARISONS wire/fields/{name}/{mode}"
        );
        let serialized = serde_json::to_value(&value).unwrap();
        let wire = safe_cex_num::js_json_stringify(&safe_cex_num::JsJsonValue::from(serialized))
            .unwrap()
            .unwrap();
        assert_eq!(
            wire,
            row["wire"].as_str().unwrap(),
            "wire/{name}/{mode}/serialization/ingress{ingress}"
        );
        compared_ingresses += 1;
    }
    assert_eq!(
        compared_ingresses, 3,
        "ORDERLY-COMPARISONS wire/ingresses/{name}/{mode}"
    );
}
fn refusal<T: DeserializeOwned + Serialize + JsNumberOutput>(row: &Value) {
    let name = row["name"].as_str().unwrap();
    let mode = row["mode"].as_str().unwrap();
    let raw = row["raw"].as_str().unwrap();
    if mode.starts_with("surrogate/") {
        let decoded = js_from_str::<T>(raw).expect("typed wire surrogate keeps the record");
        typed_utf16::assert_projection(raw, &decoded);
        return;
    }
    assert!(js_from_str::<T>(raw).is_err(), "boundary/{name}/{mode}/raw");
    match serde_json::from_str::<Value>(raw) {
        Ok(input) => {
            assert!(
                js_from_value::<T>(input.clone()).is_err(),
                "boundary/{name}/{mode}/owned"
            );
            assert!(
                js_from_value_ref::<T>(&input).is_err(),
                "boundary/{name}/{mode}/borrowed"
            );
        }
        Err(_) => assert!(
            row["mode"].as_str().unwrap().starts_with("surrogate"),
            "unexpected Value representation refusal"
        ),
    }
}

fn dispatch(row: &Value, reject: bool) {
    macro_rules! select {
        ($t:ty) => {
            if reject {
                refusal::<$t>(row)
            } else {
                compare::<$t>(row)
            }
        };
    }
    match row["name"].as_str().unwrap() {
        "ACCOUNT_RESPONSE" => select!(AccountResponse),
        "BALANCE_RESPONSE" => select!(BalanceResponse),
        "PLACE_ORDER_RESPONSE" => select!(PlaceOrderResponse),
        "EDIT_ORDER_REQUEST" => select!(EditOrderRequest),
        "EDIT_ORDER_RESPONSE" => select!(EditOrderResponse),
        unknown => panic!("unhandled actual TS type: {unknown}"),
    }
}
fn literal<T: DeserializeOwned + Serialize + PartialEq + std::fmt::Debug>(
    row: &Value,
    native: &[(&str, T)],
) {
    let name = row["name"].as_str().unwrap();
    let field = row["field"].as_str().unwrap();
    let source_values = row["values"].as_array().unwrap();
    assert_eq!(
        source_values.len(),
        native.len(),
        "ORDERLY-CARDINALITY literal/{name}{field}/count"
    );
    let mut compared_literals = 0;
    for (index, value) in source_values.iter().enumerate() {
        assert_eq!(
            value.as_str().unwrap(),
            native[index].0,
            "ORDERLY-CARDINALITY literal/{name}{field}/value"
        );
        let raw = serde_json::to_string(value).unwrap();
        let decoded: T = serde_json::from_str(&raw).unwrap();
        assert_eq!(
            decoded, native[index].1,
            "literal/{name}{field}/decoded-native"
        );
        assert_eq!(
            serde_json::to_value(decoded).unwrap(),
            *value,
            "literal/{name}{field}/native"
        );
        for (mode, malformed) in [
            ("tagged", json!({value.as_str().unwrap(): null})),
            ("array", json!([value])),
            ("null", Value::Null),
            ("unknown", json!("UNKNOWN")),
        ] {
            assert!(
                serde_json::from_value::<T>(malformed).is_err(),
                "literal/{name}{field}/{mode}"
            );
        }
        compared_literals += 1;
    }
    assert_eq!(
        compared_literals,
        native.len(),
        "ORDERLY-COMPARISONS wire/literals/{name}/{field}"
    );
}
#[test]
fn actual_typescript_wire_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/wire-oracle.cjs"
        ))
        .output()
        .expect("start one TypeScript oracle for entire wire corpus");
    assert!(
        output.status.success(),
        "wire oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["cases"].as_array().unwrap().len(),
        170,
        "ORDERLY-CARDINALITY wire/cases source total"
    );
    assert_eq!(
        corpus["refusals"].as_array().unwrap().len(),
        112,
        "ORDERLY-CARDINALITY wire/refusals source total"
    );
    assert_eq!(
        corpus["literals"].as_array().unwrap().len(),
        3,
        "ORDERLY-CARDINALITY wire/literals source total"
    );
    assert_eq!(
        corpus["projections"].as_array().unwrap().len(),
        9,
        "ORDERLY-CARDINALITY wire/projections source total"
    );
    let mut compared_groups = std::collections::BTreeMap::new();
    eprintln!("wire runtime: {}", corpus["runtime"]);
    for row in corpus["literals"].as_array().unwrap() {
        match (
            row["name"].as_str().unwrap(),
            row["field"].as_str().unwrap(),
        ) {
            ("ACCOUNT_RESPONSE", "/account_mode") => {
                literal(row, &[("FUTURES", AccountMode::Futures)])
            }
            ("EDIT_ORDER_REQUEST", "/order_type") => {
                literal(row, &[("LIMIT", EditOrderType::Limit)])
            }
            ("EDIT_ORDER_REQUEST", "/side") => literal(
                row,
                &[("BUY", EditOrderSide::Buy), ("SELL", EditOrderSide::Sell)],
            ),
            unknown => panic!("unhandled source literal {unknown:?}"),
        }
        *compared_groups
            .entry(("literals", row["name"].as_str().unwrap()))
            .or_insert(0) += 1;
    }
    for row in corpus["cases"].as_array().unwrap() {
        dispatch(row, false);
        *compared_groups
            .entry(("cases", row["name"].as_str().unwrap()))
            .or_insert(0) += 1;
    }
    for row in corpus["projections"].as_array().unwrap() {
        let path = row["extraPath"].as_str().unwrap();
        assert_eq!(
            row["retained"][path],
            json!(["string", "source retains undeclared property"]),
            "source unknown-property retention/{path}"
        );
        // Compare native declared fields and the independently observed source base;
        // the unknown source property is deliberately absent in the Rust record.
        dispatch(row, false);
        *compared_groups
            .entry(("projections", row["name"].as_str().unwrap()))
            .or_insert(0) += 1;
    }
    for row in corpus["refusals"].as_array().unwrap() {
        dispatch(row, true);
        *compared_groups
            .entry(("refusals", row["name"].as_str().unwrap()))
            .or_insert(0) += 1;
    }
    eprintln!(
        "wire corpus: {} accepted cases, {} boundary refusals; one Node process",
        corpus["cases"].as_array().unwrap().len(),
        corpus["refusals"].as_array().unwrap().len()
    );
    assert_eq!(
        compared_groups,
        std::collections::BTreeMap::from([
            (("cases", "ACCOUNT_RESPONSE"), 34),
            (("cases", "BALANCE_RESPONSE"), 63),
            (("cases", "EDIT_ORDER_REQUEST"), 20),
            (("cases", "EDIT_ORDER_RESPONSE"), 20),
            (("cases", "PLACE_ORDER_RESPONSE"), 33),
            (("projections", "ACCOUNT_RESPONSE"), 1),
            (("projections", "BALANCE_RESPONSE"), 3),
            (("projections", "EDIT_ORDER_REQUEST"), 1),
            (("projections", "EDIT_ORDER_RESPONSE"), 2),
            (("projections", "PLACE_ORDER_RESPONSE"), 2),
            (("refusals", "ACCOUNT_RESPONSE"), 25),
            (("refusals", "BALANCE_RESPONSE"), 32),
            (("refusals", "EDIT_ORDER_REQUEST"), 20),
            (("refusals", "EDIT_ORDER_RESPONSE"), 12),
            (("refusals", "PLACE_ORDER_RESPONSE"), 23),
            (("literals", "ACCOUNT_RESPONSE"), 1),
            (("literals", "EDIT_ORDER_REQUEST"), 2)
        ]),
        "ORDERLY-COMPARISONS wire/per-type groups"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
