use safe_cex_num::numeric_ingress::{
    JsNumberOutput, js_from_str, js_from_value, js_from_value_ref,
};
use safe_cex_types::{
    Balance, ExchangeOptions, Market, Order, OrderBook, PlaceOrderOpts, Position, Ticker,
};
use serde::{Serialize, de::DeserializeOwned};
use serde_json::Value;

// Supplemental field-identity evidence; existing numeric-edge corpus bytes stay intact.
fn entries<T: DeserializeOwned + Serialize + JsNumberOutput>(raw: &str) -> [T; 3] {
    let value: Value = serde_json::from_str(raw).unwrap();
    [
        js_from_str(raw).unwrap(),
        js_from_value(value.clone()).unwrap(),
        js_from_value_ref(&value).unwrap(),
    ]
}
fn numbers(actual: &[f64], expected: &[f64], name: &str) {
    assert_eq!(actual.len(), expected.len(), "FIELD IDENTITY {name} count");
    for (i, (actual, expected)) in actual.iter().zip(expected).enumerate() {
        assert_eq!(
            actual.to_bits(),
            expected.to_bits(),
            "FIELD IDENTITY {name}[{i}]"
        );
    }
}
#[test]
fn placement_book_and_tuple_native_field_identity_supplement() {
    for value in entries::<PlaceOrderOpts>(
        r#"{"symbol":"BTC","type":"limit","side":"buy","amount":1,"price":2,"triggerPrice":3,"stopLoss":4,"takeProfit":5,"maxSlippagePct":6}"#,
    ) {
        numbers(
            &[
                value.amount,
                value.price.unwrap(),
                value.trigger_price.unwrap(),
                value.stop_loss.unwrap(),
                value.take_profit.unwrap(),
                value.max_slippage_pct.unwrap(),
            ],
            &[1., 2., 3., 4., 5., 6.],
            "placement",
        );
    }
    for value in entries::<OrderBook>(
        r#"{"bids":[{"price":1,"amount":2,"total":3},{"price":4,"amount":5,"total":6}],"asks":[{"price":7,"amount":8,"total":9},{"price":10,"amount":11,"total":12}]}"#,
    ) {
        let actual: Vec<_> = value
            .bids
            .iter()
            .chain(&value.asks)
            .flat_map(|v| [v.price, v.amount, v.total])
            .collect();
        numbers(
            &actual,
            &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11., 12.],
            "book sides and rows",
        );
    }
    for value in entries::<(f64, (Option<f64>, Vec<f64>))>("[1,[2,[3,4]]]") {
        numbers(
            &[value.0, value.1.0.unwrap(), value.1.1[0], value.1.1[1]],
            &[1., 2., 3., 4.],
            "tuple",
        );
    }
}
#[test]
fn ordered_map_native_value_identity_supplement() {
    let raw = r#"{"10":1,"2":2,"z":3,"a":4}"#;
    for value in entries::<safe_cex_types::object_map::JsObjectMap<f64>>(raw) {
        numbers(
            &[value["10"], value["2"], value["z"], value["a"]],
            &[1., 2., 3., 4.],
            "JsObjectMap key values",
        );
    }
    for value in entries::<indexmap::IndexMap<String, f64>>(raw) {
        numbers(
            &[value["10"], value["2"], value["z"], value["a"]],
            &[1., 2., 3., 4.],
            "IndexMap key values",
        );
    }
}
#[test]
fn domain_native_field_identity_supplement() {
    for v in entries::<Market>(
        r#"{"id":"market-id","symbol":"market-symbol","base":"B","quote":"Q","active":true,"precision":{"amount":1,"price":2},"limits":{"amount":{"min":3,"max":4},"leverage":{"min":5,"max":6}}}"#,
    ) {
        assert_eq!(
            (&*v.id, &*v.symbol, &*v.base, &*v.quote),
            ("market-id", "market-symbol", "B", "Q"),
            "FIELD IDENTITY market strings"
        );
        numbers(
            &[
                v.precision.amount,
                v.precision.price,
                v.limits.amount.min,
                safe_cex_num::numeric_value_to_number(&v.limits.amount.max),
                v.limits.leverage.min,
                v.limits.leverage.max,
            ],
            &[1., 2., 3., 4., 5., 6.],
            "market numbers",
        );
    }
    for v in entries::<Order>(
        r#"{"id":"order-id","symbol":"order-symbol","status":"open","type":"limit","side":"buy","price":1,"amount":2,"filled":3,"remaining":4,"reduceOnly":true,"lastUpdate":5}"#,
    ) {
        assert_eq!(
            (&*v.id, &*v.symbol),
            ("order-id", "order-symbol"),
            "FIELD IDENTITY order strings"
        );
        numbers(
            &[
                v.price.to_number(),
                v.amount,
                v.filled,
                v.remaining,
                v.last_update,
            ],
            &[1., 2., 3., 4., 5.],
            "order numbers",
        );
    }
    for v in entries::<Position>(
        r#"{"id":"position-id","symbol":"position-symbol","side":"long","entryPrice":1,"notional":2,"leverage":3,"unrealizedPnl":4,"contracts":5,"liquidationPrice":6,"lastUpdate":7}"#,
    ) {
        assert_eq!(
            (&*v.id, &*v.symbol),
            ("position-id", "position-symbol"),
            "FIELD IDENTITY position strings"
        );
        numbers(
            &[
                v.entry_price,
                v.notional,
                v.leverage,
                v.unrealized_pnl,
                v.contracts,
                v.liquidation_price,
                v.last_update.unwrap(),
            ],
            &[1., 2., 3., 4., 5., 6., 7.],
            "position numbers",
        );
    }
    for v in entries::<Ticker>(
        r#"{"id":"ticker-id","symbol":"ticker-symbol","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"openInterest":7,"fundingRate":8,"fundingTime":9,"volume":10,"quoteVolume":11}"#,
    ) {
        assert_eq!(
            (&*v.id, &*v.symbol),
            ("ticker-id", "ticker-symbol"),
            "FIELD IDENTITY ticker strings"
        );
        numbers(
            &[
                v.bid,
                v.ask,
                v.last,
                v.mark,
                v.index.unwrap(),
                v.percentage,
                v.open_interest,
                v.funding_rate,
                v.funding_time,
                v.volume,
                v.quote_volume,
            ],
            &[1., 2., 3., 4., 5., 6., 7., 8., 9., 10., 11.],
            "ticker numbers",
        );
    }
    for v in entries::<Balance>(r#"{"used":1,"free":2,"total":3,"upnl":4}"#) {
        numbers(
            &[v.used, v.free, v.total, v.upnl],
            &[1., 2., 3., 4.],
            "balance numbers",
        );
    }
    for v in entries::<ExchangeOptions>(r#"{"key":"api-key","secret":"api-secret"}"#) {
        assert_eq!(
            (&*v.key, &*v.secret),
            ("api-key", "api-secret"),
            "FIELD IDENTITY config credentials"
        );
    }
}

#[test]
fn recursive_visits_keep_element_identity_supplement() {
    use std::cell::Cell;
    struct Visit<'a>(&'a Cell<usize>);
    impl JsNumberOutput for Visit<'_> {
        fn check_number_output(&self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let cells: [Cell<usize>; 16] = std::array::from_fn(|_| Cell::new(0));
    let values: [Visit<'_>; 16] = std::array::from_fn(|i| Visit(&cells[i]));
    macro_rules! check_tuple {
        ($($i:literal),+) => {{
            for cell in &cells { cell.set(0); }
            ($(&values[$i],)+).check_number_output();
            let indices = [$($i),+];
            for (i, cell) in cells.iter().enumerate() {
                assert_eq!(cell.get(), usize::from(indices.contains(&i)), "TRAVERSAL FIELD IDENTITY tuple element {i}");
            }
        }};
    }
    check_tuple!(0);
    check_tuple!(0, 1);
    check_tuple!(0, 1, 2);
    check_tuple!(0, 1, 2, 3);
    check_tuple!(0, 1, 2, 3, 4);
    check_tuple!(0, 1, 2, 3, 4, 5);
    check_tuple!(0, 1, 2, 3, 4, 5, 6);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14);
    check_tuple!(0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15);
    for cell in &cells {
        cell.set(0);
    }
    let object: safe_cex_types::object_map::JsObjectMap<_> =
        [("a".into(), &values[0]), ("b".into(), &values[1])]
            .into_iter()
            .collect();
    object.check_number_output();
    assert_eq!(
        [cells[0].get(), cells[1].get()],
        [1, 1],
        "TRAVERSAL FIELD IDENTITY object values"
    );
    let index: indexmap::IndexMap<_, _> =
        [("a", &values[2]), ("b", &values[3])].into_iter().collect();
    index.check_number_output();
    assert_eq!(
        [cells[2].get(), cells[3].get()],
        [1, 1],
        "TRAVERSAL FIELD IDENTITY index values"
    );
    let nested: Box<[Option<Vec<&Visit<'_>>>]> =
        vec![Some(vec![&values[4], &values[5]]), None].into_boxed_slice();
    nested.check_number_output();
    assert_eq!(
        [cells[4].get(), cells[5].get()],
        [1, 1],
        "TRAVERSAL FIELD IDENTITY boxed slice"
    );
}

#[derive(serde::Deserialize, Serialize)]
struct OpaqueIdentity {
    n: f64,
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    raw_response: Value,
}
impl JsNumberOutput for OpaqueIdentity {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { n } passthrough { raw_response });
    }
}
#[derive(serde::Deserialize, Serialize)]
#[serde(tag = "kind")]
enum TaggedOpaqueIdentity {
    Data(OpaqueIdentity),
}
impl JsNumberOutput for TaggedOpaqueIdentity {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Data(value) });
    }
}
#[test]
fn typed_and_opaque_native_field_identity_supplement() {
    let raw = r#"{"n":3,"raw_response":{"nested":[7]}}"#;
    let mut values = entries::<OpaqueIdentity>(raw)
        .into_iter()
        .collect::<Vec<_>>();
    for TaggedOpaqueIdentity::Data(value) in
        entries::<TaggedOpaqueIdentity>(r#"{"kind":"Data","n":3,"raw_response":{"nested":[7]}}"#)
    {
        values.push(value);
    }
    for value in values {
        numbers(
            &[
                value.n,
                safe_cex_num::numeric_ingress::js_value_as_f64(&value.raw_response["nested"][0])
                    .unwrap(),
            ],
            &[3., 7.],
            "typed versus opaque native numbers",
        );
    }
}

#[test]
fn retained_input_collection_cardinality_supplement() {
    for value in entries::<Vec<f64>>("[3,7]") {
        numbers(&value, &[3., 7.], "captured array cardinality");
    }
    for value in entries::<std::collections::BTreeMap<String, f64>>(r#"{"left":3,"right":7}"#) {
        assert_eq!(value.len(), 2, "CAPTURE-CARDINALITY native map");
        numbers(
            &[value["left"], value["right"]],
            &[3., 7.],
            "captured map members",
        );
    }
    let mut compared = 0;
    for raw in [r#"[3.0,7.0]"#, r#"{"left":3.0,"right":7.0}"#] {
        let expected: Value = serde_json::from_str(raw).unwrap();
        for value in entries::<safe_cex_num::numeric_ingress::JsNumberInput>(raw) {
            let actual = serde_json::to_value(&value).unwrap();
            let (native, wire) = if expected.is_array() {
                let native = value.decode::<Vec<f64>>().unwrap();
                let wire = actual.as_array().unwrap();
                assert_eq!(wire.len(), 2, "CAPTURE-CARDINALITY opaque wire array");
                (
                    native,
                    wire.iter().map(|v| v.as_f64().unwrap()).collect::<Vec<_>>(),
                )
            } else {
                let native = value
                    .decode::<std::collections::BTreeMap<String, f64>>()
                    .unwrap();
                let wire = actual.as_object().unwrap();
                assert_eq!(native.len(), 2, "CAPTURE-CARDINALITY opaque native map");
                assert_eq!(
                    wire.keys().collect::<Vec<_>>(),
                    expected.as_object().unwrap().keys().collect::<Vec<_>>(),
                    "CAPTURE-CARDINALITY opaque wire keys"
                );
                (
                    vec![native["left"], native["right"]],
                    vec![
                        wire["left"].as_f64().unwrap(),
                        wire["right"].as_f64().unwrap(),
                    ],
                )
            };
            numbers(&native, &[3., 7.], "opaque native numeric identity");
            numbers(&wire, &[3., 7.], "opaque serialized numeric identity");
            compared += native.len();
        }
    }
    assert_eq!(compared, 12, "CAPTURE-CARDINALITY opaque comparisons");
}

#[test]
fn opaque_passthrough_collection_cardinality_supplement() {
    let raw = r#"{"n":3,"raw_response":{"nested":[7,11],"other":[13,17]}}"#;
    let mut values = entries::<OpaqueIdentity>(raw)
        .into_iter()
        .collect::<Vec<_>>();
    for TaggedOpaqueIdentity::Data(value) in entries::<TaggedOpaqueIdentity>(
        r#"{"kind":"Data","n":3,"raw_response":{"nested":[7,11],"other":[13,17]}}"#,
    ) {
        values.push(value);
    }
    assert_eq!(values.len(), 6, "PASSTHROUGH-CARDINALITY entry routes");
    let mut compared = 0;
    for value in values {
        let object = value.raw_response.as_object().unwrap();
        assert_eq!(object.len(), 2, "PASSTHROUGH-CARDINALITY native members");
        for (key, expected) in [("nested", [7., 11.]), ("other", [13., 17.])] {
            let array = object[key].as_array().unwrap();
            assert_eq!(array.len(), 2, "PASSTHROUGH-CARDINALITY native array {key}");
            let actual: Vec<_> = array
                .iter()
                .map(|value| safe_cex_num::numeric_ingress::js_value_as_f64(value).unwrap())
                .collect();
            numbers(&actual, &expected, key);
            compared += actual.len();
        }
    }
    assert_eq!(compared, 24, "PASSTHROUGH-CARDINALITY compared numbers");
}
