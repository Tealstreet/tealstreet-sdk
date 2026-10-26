#![cfg(not(target_arch = "wasm32"))]
use safe_cex_num::numeric_ingress::{
    JsNumberInput, JsNumberOutput, js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
};
use serde::{Deserialize, Deserializer, Serialize, de::DeserializeOwned};
use serde_json::{Value, json};

#[derive(Debug, Deserialize, Serialize)]
struct Plain {
    n: f64,
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind")]
enum Tagged {
    Number { n: f64 },
}
#[derive(Debug, Deserialize, Serialize)]
#[serde(untagged)]
enum Untagged {
    Number(f64),
    Text(String),
}
#[derive(Debug, Deserialize, Serialize)]
struct Flattened {
    label: String,
    #[serde(flatten)]
    value: Plain,
}
#[derive(Debug, Deserialize, Serialize)]
struct Generic<T> {
    data: T,
}

#[derive(Debug, Serialize)]
struct Buffered(Plain);
impl<'de> Deserialize<'de> for Buffered {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                Plain::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map(Self)
            .map_err(serde::de::Error::custom)
    }
}
#[derive(Debug, Serialize)]
struct Nested {
    data: Buffered,
}
impl<'de> Deserialize<'de> for Nested {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input = JsNumberInput::deserialize(deserializer)?;
        input
            .decode_with(|decoder| {
                Generic::<Buffered>::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map(|v| Self { data: v.data })
            .map_err(serde::de::Error::custom)
    }
}

impl JsNumberOutput for Plain {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { n });
    }
}
impl JsNumberOutput for Tagged {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Number { n } });
    }
}
impl JsNumberOutput for Untagged {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Number(n), Text(s) });
    }
}
impl JsNumberOutput for Flattened {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { label, value });
    }
}
impl<T: JsNumberOutput> JsNumberOutput for Generic<T> {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { data });
    }
}
impl JsNumberOutput for Buffered {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, tuple(value));
    }
}
impl JsNumberOutput for Nested {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { data });
    }
}

fn entries<T: DeserializeOwned + Serialize + JsNumberOutput>(raw: &str) -> [T; 3] {
    let value: Value = serde_json::from_str(raw).unwrap();
    [
        js_from_str(raw).unwrap(),
        js_from_value(value.clone()).unwrap(),
        js_from_value_ref(&value).unwrap(),
    ]
}
fn bit(number: f64, expected: u64, path: &str, raw: &str) {
    assert_eq!(number.to_bits(), expected, "numeric ingress {path}: {raw}");
}

fn oracle_rows() -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/numeric_ingress.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "Node numeric oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 2019, "numeric ingress oracle census");
    rows
}

#[test]
fn whole_path_numbers_match_live_node_bits() {
    let rows = oracle_rows();
    for row in rows {
        let text = row["text"].as_str().unwrap();
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        for n in entries::<f64>(text) {
            bit(n, expected, "scalar", text);
        }
        for plain in entries::<Plain>(&format!("{{\"n\":{text}}}")) {
            bit(plain.n, expected, "plain positive control", text);
        }
        for tagged in entries::<Tagged>(&format!("{{\"kind\":\"Number\",\"n\":{text}}}")) {
            let Tagged::Number { n } = tagged;
            bit(n, expected, "tagged", text);
        }
        for union in entries::<Untagged>(text) {
            match union {
                Untagged::Number(n) => bit(n, expected, "untagged", text),
                Untagged::Text(s) => panic!("numeric ingress became text: {s}"),
            }
        }
        for flat in entries::<Flattened>(&format!("{{\"label\":\"keep\",\"n\":{text}}}")) {
            assert_eq!(flat.label, "keep");
            bit(flat.value.n, expected, "flattened", text);
        }
        for nested in entries::<Nested>(&format!("{{\"data\":{{\"n\":{text}}}}}")) {
            bit(nested.data.0.n, expected, "nested second buffer", text);
        }
        let generic = format!("{{\"data\":{text}}}");
        for typed in entries::<Generic<f64>>(&generic) {
            bit(typed.data, expected, "generic typed positive control", text);
        }
        for buffered in entries::<Generic<JsNumberInput>>(&generic) {
            bit(
                buffered.data.decode::<f64>().unwrap(),
                expected,
                "generic shared JSON buffer",
                text,
            );
        }
        let value: Value = serde_json::from_str(text).unwrap();
        bit(
            js_value_as_f64(&value).unwrap(),
            expected,
            "explicit JSON number accessor",
            text,
        );
    }
}

#[test]
fn existing_clean_owned_and_native_negative_zero_stay_clean() {
    for value in [
        serde_json::from_str::<Value>("-0").unwrap(),
        serde_json::from_str("-0.0").unwrap(),
        json!(-0.0),
    ] {
        let before = serde_json::from_value::<f64>(value.clone()).unwrap();
        assert_eq!(
            before.to_bits(),
            (-0.0_f64).to_bits(),
            "existing clean path"
        );
        bit(
            js_from_value::<f64>(value.clone()).unwrap(),
            before.to_bits(),
            "clean owned regression",
            "-0",
        );
        bit(
            js_from_value_ref::<f64>(&value).unwrap(),
            before.to_bits(),
            "clean borrowed regression",
            "-0",
        );
    }
}

#[test]
fn negative_control_native_infinity_value_visitor_projects_to_null() {
    // Deliberately unsafe predecessor: this proves the stock Value visitor
    // projects infinity to null. Do not "repair" this negative control.
    for number in [f64::INFINITY, f64::NEG_INFINITY] {
        let projected = Value::deserialize(
            serde::de::value::F64Deserializer::<serde_json::Error>::new(number),
        )
        .unwrap();
        assert!(
            projected.is_null(),
            "negative control native Infinity projects to null"
        );
    }
}

#[test]
fn numeric_ingress_does_not_coerce_other_json_shapes() {
    for raw in [
        "null",
        "true",
        "false",
        "\"1\"",
        "[]",
        "{}",
        "{\"$serde_json::private::Number\":\"1.5\"}",
        "{\"$serde_json::private::RawValue\":\"1.5\"}",
    ] {
        assert!(
            js_from_str::<f64>(raw).is_err(),
            "numeric shape refusal {raw}"
        );
    }
    for raw in ["NaN", "Infinity", "01", "+1", "1.", "1e", "1 2"] {
        assert!(
            js_from_str::<f64>(raw).is_err(),
            "numeric syntax refusal {raw}"
        );
    }
    for value in [Value::Null, json!(false), json!("1"), json!([]), json!({})] {
        assert!(js_value_as_f64(&value).is_none());
    }
    assert!(
        js_from_str::<Buffered>("[1]").is_err(),
        "shared map-only ingress remains strict"
    );
}

#[test]
fn numeric_integer_targets_do_not_retain_precision_absent_from_js_number() {
    for decoded in entries::<u64>("9007199254740993") {
        assert_eq!(
            decoded, 9007199254740992,
            "Number rounds before integer target"
        );
    }
    for decoded in entries::<String>(r#""9007199254740993""#) {
        assert_eq!(
            decoded, "9007199254740993",
            "string input retains exact spelling"
        );
    }
}

#[test]
fn raw_capture_recursion_limit_matches_standard_json_on_every_entry() {
    for depth in [0, 1, 126, 127, 128, 129, 200, 256] {
        let raw = format!("{}0{}", "[".repeat(depth), "]".repeat(depth));
        let baseline = serde_json::from_str::<Value>(&raw);
        let expected = baseline.is_ok();
        let actual = js_from_str::<JsNumberInput>(&raw);
        assert_eq!(
            actual.is_ok(),
            expected,
            "raw capture recursion boundary at {depth}"
        );
        let mut value = json!(0);
        for _ in 0..depth {
            value = Value::Array(vec![value]);
        }
        assert_eq!(
            js_from_value_ref::<JsNumberInput>(&value).is_ok(),
            expected,
            "borrowed recursion boundary at {depth}"
        );
        assert_eq!(
            js_from_value::<JsNumberInput>(value).is_ok(),
            expected,
            "owned recursion boundary at {depth}"
        );
    }
}

#[test]
fn value_output_eligibility_controls_fail_with_named_trait_diagnostics() {
    let cases = trybuild::TestCases::new();
    cases.pass("tests/numeric_ingress_ui/shared_buffer.rs");
    cases.compile_fail("tests/numeric_ingress_ui/generic_value.rs");
    cases.compile_fail("tests/numeric_ingress_ui/skipped_value.rs");
    cases.compile_fail("tests/numeric_ingress_ui/renamed_value.rs");
    cases.compile_fail("tests/numeric_ingress_ui/omitted_field.rs");
}

#[derive(Debug, Serialize, Deserialize)]
struct SharedPlacement {
    #[serde(flatten)]
    options: safe_cex_types::PlaceOrderOpts,
}
impl JsNumberOutput for SharedPlacement {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { options });
    }
}
#[derive(Debug, Serialize, Deserialize)]
struct SharedBook {
    #[serde(flatten)]
    book: safe_cex_types::OrderBook,
}
impl JsNumberOutput for SharedBook {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { book });
    }
}

#[test]
fn shared_models_preserve_every_placement_and_book_number() {
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let expected =
            f64::from_bits(u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap());
        let raw = format!(
            r#"{{"symbol":"BTC","type":"limit","side":"buy","amount":{spelling},"price":{spelling},"triggerPrice":{spelling},"stopLoss":{spelling},"takeProfit":{spelling},"maxSlippagePct":{spelling}}}"#
        );
        let value: Value = serde_json::from_str(&raw).unwrap();
        for p in [
            js_from_str::<SharedPlacement>(&raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
        ] {
            for n in [
                p.options.amount,
                p.options.price.unwrap(),
                p.options.trigger_price.unwrap(),
                p.options.stop_loss.unwrap(),
                p.options.take_profit.unwrap(),
                p.options.max_slippage_pct.unwrap(),
            ] {
                assert_eq!(
                    n.to_bits(),
                    expected.to_bits(),
                    "shared placement number {spelling}"
                );
            }
        }
        let raw = format!(
            r#"{{"bids":[{{"price":{spelling},"amount":{spelling},"total":{spelling}}}],"asks":[{{"price":{spelling},"amount":{spelling},"total":{spelling}}}]}}"#
        );
        let value: Value = serde_json::from_str(&raw).unwrap();
        for p in [
            js_from_str::<SharedBook>(&raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
        ] {
            assert_eq!(p.book.bids.len(), 1, "INGRESS-CARDINALITY native bids");
            assert_eq!(p.book.asks.len(), 1, "INGRESS-CARDINALITY native asks");
            let mut compared = 0;
            for row in p.book.bids.iter().chain(&p.book.asks) {
                for n in [row.price, row.amount, row.total] {
                    assert_eq!(
                        n.to_bits(),
                        expected.to_bits(),
                        "shared orderbook number {spelling}"
                    );
                    compared += 1;
                }
            }
            assert_eq!(compared, 6, "INGRESS-CARDINALITY compared book numbers");
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
struct OpaqueResponse {
    n: f64,
    #[serde(default, deserialize_with = "opaque_present")]
    raw_response: Option<Value>,
}
fn opaque_present<'de, D: Deserializer<'de>>(d: D) -> Result<Option<Value>, D::Error> {
    safe_cex_num::numeric_ingress::js_deserialize_passthrough(d).map(Some)
}
impl JsNumberOutput for OpaqueResponse {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { n } passthrough { raw_response });
    }
}
#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind")]
enum TaggedOpaque {
    Response(OpaqueResponse),
}
impl JsNumberOutput for TaggedOpaque {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Response(value) });
    }
}

#[test]
fn declared_passthrough_preserves_tagged_overflow_and_signed_zero() {
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let expected =
            f64::from_bits(u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap());
        let raw = format!(
            r#"{{"kind":"Response","n":{spelling},"raw_response":{{"nested":[{spelling}]}}}}"#
        );
        let value: Value = serde_json::from_str(&raw).unwrap();
        for output in [
            js_from_str::<TaggedOpaque>(&raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
        ] {
            let TaggedOpaque::Response(output) = output;
            assert_eq!(output.n.to_bits(), expected.to_bits());
            assert_eq!(
                js_value_as_f64(&output.raw_response.unwrap()["nested"][0])
                    .unwrap()
                    .to_bits(),
                expected.to_bits(),
                "declared tagged passthrough {spelling}"
            );
        }
    }
}

#[test]
fn declared_passthrough_does_not_disable_native_infinity_projection_guard() {
    // Negative control: naming a passthrough without using its capture helper
    // lets the stock Value visitor project native infinity to null. The retained
    // whole-output guard MUST refuse this, rather than treating it as an escape.
    #[derive(Debug, Serialize, Deserialize)]
    struct Wrong {
        raw_response: Option<Value>,
    }
    impl JsNumberOutput for Wrong {
        fn check_number_output(&self) {
            safe_cex_num::js_number_output!(self, struct {} passthrough { raw_response });
        }
    }
    #[derive(Debug, Serialize, Deserialize)]
    #[serde(tag = "kind")]
    enum WrongTagged {
        Response(Wrong),
    }
    impl JsNumberOutput for WrongTagged {
        fn check_number_output(&self) {
            safe_cex_num::js_number_output!(self, enum { Response(value) });
        }
    }
    let error = js_from_str::<WrongTagged>(r#"{"kind":"Response","raw_response":{"n":1e400}}"#)
        .unwrap_err();
    assert!(
        error
            .to_string()
            .contains("numeric ingress lost Number at $.raw_response.n"),
        "declared passthrough projection guard: {error}"
    );
}

#[test]
fn tuple_recursion_preserves_numbers() {
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        for pair in entries::<(f64, (Option<f64>, Vec<f64>))>(&format!(
            "[{spelling},[{spelling},[{spelling}]]]"
        )) {
            bit(pair.0, expected, "tuple first", spelling);
            bit(pair.1.0.unwrap(), expected, "nested tuple first", spelling);
            bit(pair.1.1[0], expected, "nested tuple second", spelling);
        }
    }
}

#[derive(Debug, Deserialize, Serialize)]
struct OrderedMaps {
    object: safe_cex_types::object_map::JsObjectMap<f64>,
    index: indexmap::IndexMap<String, f64>,
}
impl JsNumberOutput for OrderedMaps {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { object, index });
    }
}
#[test]
fn ordered_map_records_preserve_node_bits_and_key_order() {
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let expected = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        let map = format!(r#"{{"10":{spelling},"2":{spelling},"z":{spelling},"a":{spelling}}}"#);
        for mut record in entries::<OrderedMaps>(&format!(r#"{{"object":{map},"index":{map}}}"#)) {
            assert_eq!(
                record.object.keys().map(String::as_str).collect::<Vec<_>>(),
                ["2", "10", "z", "a"],
                "JsObjectMap canonical index order"
            );
            assert_eq!(
                record.index.keys().map(String::as_str).collect::<Vec<_>>(),
                ["10", "2", "z", "a"],
                "IndexMap insertion order unchanged"
            );
            for number in record.object.values().chain(record.index.values()) {
                bit(*number, expected, "ordered map payload", spelling);
            }
            record.object.insert("z".into(), 1.0);
            record.object.remove("z");
            record.object.insert("z".into(), 2.0);
            record.check_number_output();
            assert_eq!(
                record.object.keys().map(String::as_str).collect::<Vec<_>>(),
                ["2", "10", "a", "z"],
                "eligibility does not change reinsertion order"
            );
        }
    }
}

#[test]
fn container_recursion_visits_every_element() {
    use std::cell::Cell;
    struct Count<'a>(&'a Cell<usize>);
    impl JsNumberOutput for Count<'_> {
        fn check_number_output(&self) {
            self.0.set(self.0.get() + 1);
        }
    }
    let count = Cell::new(0);
    let value = Count(&count);
    let object: safe_cex_types::object_map::JsObjectMap<_> =
        [("a".into(), &value), ("b".into(), &value)]
            .into_iter()
            .collect();
    object.check_number_output();
    assert_eq!(count.get(), 2, "JsObjectMap visits every value");
    let index: indexmap::IndexMap<_, _> = [("a", &value), ("b", &value)].into_iter().collect();
    index.check_number_output();
    assert_eq!(count.get(), 4, "IndexMap visits every value");
    let nested: Box<[Option<Vec<&Count<'_>>>]> =
        vec![Some(vec![&value, &value]), None].into_boxed_slice();
    nested.check_number_output();
    assert_eq!(count.get(), 6, "Box slice Option Vec recursion");
    let array = [&value, &value];
    array.check_number_output();
    assert_eq!(count.get(), 8, "array recursion");
    count.set(0);
    (&value,).check_number_output();
    assert_eq!(count.get(), 1, "tuple arity 1 visits every element");
    count.set(0);
    (&value, &value).check_number_output();
    assert_eq!(count.get(), 2, "tuple arity 2 visits every element");
    count.set(0);
    (&value, &value, &value).check_number_output();
    assert_eq!(count.get(), 3, "tuple arity 3 visits every element");
    count.set(0);
    (&value, &value, &value, &value).check_number_output();
    assert_eq!(count.get(), 4, "tuple arity 4 visits every element");
    count.set(0);
    (&value, &value, &value, &value, &value).check_number_output();
    assert_eq!(count.get(), 5, "tuple arity 5 visits every element");
    count.set(0);
    (&value, &value, &value, &value, &value, &value).check_number_output();
    assert_eq!(count.get(), 6, "tuple arity 6 visits every element");
    count.set(0);
    (&value, &value, &value, &value, &value, &value, &value).check_number_output();
    assert_eq!(count.get(), 7, "tuple arity 7 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 8, "tuple arity 8 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 9, "tuple arity 9 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 10, "tuple arity 10 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 11, "tuple arity 11 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
        &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 12, "tuple arity 12 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
        &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 13, "tuple arity 13 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
        &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 14, "tuple arity 14 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
        &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 15, "tuple arity 15 visits every element");
    count.set(0);
    (
        &value, &value, &value, &value, &value, &value, &value, &value, &value, &value, &value,
        &value, &value, &value, &value, &value,
    )
        .check_number_output();
    assert_eq!(count.get(), 16, "tuple arity 16 visits every element");
}

#[test]
fn shared_configuration_eligibility_keeps_capabilities_uninhabited() {
    use safe_cex_types::store_model::{SliceAction, TaskOrigin, TwapStatus};
    use safe_cex_types::{ExchangeOptions, ExchangeOptionsExternalPublicData};
    for value in [
        TaskOrigin::Web,
        TaskOrigin::Cli,
        TaskOrigin::Webhook,
        TaskOrigin::Automation,
    ] {
        value.check_number_output();
    }
    for value in [
        TwapStatus::Idle,
        TwapStatus::Running,
        TwapStatus::Paused,
        TwapStatus::Completed,
    ] {
        value.check_number_output();
    }
    for value in [SliceAction::Market, SliceAction::Next] {
        value.check_number_output();
    }
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        let raw = format!(
            r#"{{"key":"","secret":"","tuning":{{"dataUpdateThrottleMultiplier":{spelling}}},"externalPublicData":{{"type":"websocket","url":"wss://example.test","exchangeId":"test"}}}}"#
        );
        let value: Value = serde_json::from_str(&raw).unwrap();
        for options in [
            js_from_str::<ExchangeOptions>(&raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
        ] {
            assert_eq!(
                options
                    .tuning
                    .unwrap()
                    .data_update_throttle_multiplier
                    .unwrap()
                    .to_bits(),
                bits,
                "shared ExchangeOptions nested numeric bits {spelling}"
            );
            assert!(matches!(
                options.external_public_data,
                Some(ExchangeOptionsExternalPublicData::Variant0(_))
            ));
        }
    }
    for field in ["remoteSigner", "payloadSigner", "transport"] {
        for payload in ["null", "{}", "[]", "true", "0", "\"handle\""] {
            let raw = format!(r#"{{"key":"","secret":"","{field}":{payload}}}"#);
            assert!(
                js_from_str::<ExchangeOptions>(&raw).is_err(),
                "uninhabited capability refusal {field} {payload}"
            );
        }
    }
    for payload in ["null", "{}", "[]", "true", "0", "\"handle\""] {
        let raw = format!(r#"{{"type":"in-process","source":{payload}}}"#);
        assert!(
            js_from_str::<ExchangeOptionsExternalPublicData>(&raw).is_err(),
            "uninhabited public-data source refusal {payload}"
        );
    }
}

#[derive(Debug, Deserialize, Serialize)]
#[serde(tag = "kind")]
enum SharedInfoTagged<T> {
    Data { value: T },
}
impl<T: JsNumberOutput> JsNumberOutput for SharedInfoTagged<T> {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, enum { Data { value } });
    }
}

#[test]
fn shared_domain_info_retains_bits_on_direct_and_tagged_entries() {
    fn measure<T: DeserializeOwned + Serialize + JsNumberOutput>(
        raw: &str,
        bits: u64,
        info: fn(&T) -> Option<&safe_cex_types::retained_json::RetainedJsonInput>,
    ) {
        let value: Value = serde_json::from_str(raw).unwrap();
        for result in [
            js_from_str::<T>(raw),
            js_from_value(value.clone()),
            js_from_value_ref(&value),
        ] {
            let output = result.unwrap();
            assert_eq!(
                info(&output)
                    .unwrap()
                    .get("nested")
                    .unwrap()
                    .as_array()
                    .unwrap()[0]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                bits,
                "shared info direct bits {} {raw}",
                std::any::type_name::<T>()
            );
        }
        let tagged = format!(r#"{{"kind":"Data","value":{raw}}}"#);
        let value: Value = serde_json::from_str(&tagged).unwrap();
        for result in [
            js_from_str::<SharedInfoTagged<T>>(&tagged),
            js_from_value(value.clone()),
            js_from_value_ref(&value),
        ] {
            let SharedInfoTagged::Data { value: output } = result.unwrap();
            assert_eq!(
                info(&output)
                    .unwrap()
                    .get("nested")
                    .unwrap()
                    .as_array()
                    .unwrap()[0]
                    .as_f64()
                    .unwrap()
                    .to_bits(),
                bits,
                "shared info tagged bits {} {raw}",
                std::any::type_name::<T>()
            );
        }
        let mut absent: Value = serde_json::from_str(raw).unwrap();
        absent.as_object_mut().unwrap().remove("info");
        for present in [false, true] {
            if present {
                absent["info"] = Value::Null;
            }
            let raw = serde_json::to_string(&absent).unwrap();
            let expected = if present {
                Some(safe_cex_types::retained_json::RetainedJsonInput::Null)
            } else {
                None
            };
            for output in [
                js_from_str::<T>(&raw).unwrap(),
                js_from_value(absent.clone()).unwrap(),
                js_from_value_ref(&absent).unwrap(),
            ] {
                assert_eq!(
                    info(&output),
                    expected.as_ref(),
                    "shared info absence/null direct"
                );
            }
            let tagged = format!(r#"{{"kind":"Data","value":{raw}}}"#);
            let value: Value = serde_json::from_str(&tagged).unwrap();
            for output in [
                js_from_str::<SharedInfoTagged<T>>(&tagged).unwrap(),
                js_from_value(value.clone()).unwrap(),
                js_from_value_ref(&value).unwrap(),
            ] {
                let SharedInfoTagged::Data { value: output } = output;
                assert_eq!(
                    info(&output),
                    expected.as_ref(),
                    "shared info absence/null tagged"
                );
            }
        }
    }
    for row in oracle_rows() {
        let spelling = row["text"].as_str().unwrap();
        let bits = u64::from_str_radix(row["bits"].as_str().unwrap(), 16).unwrap();
        let info = format!(r#", "info":{{"nested":[{spelling}]}}"#);
        measure::<safe_cex_types::Market>(
            &format!(
                r#"{{"id":"x","symbol":"x","base":"B","quote":"Q","active":true,"precision":{{"amount":1,"price":1}},"limits":{{"amount":{{"min":1,"max":2}},"leverage":{{"min":1,"max":2}}}}{info}}}"#
            ),
            bits,
            |v| v.info.as_ref(),
        );
        measure::<safe_cex_types::Order>(
            &format!(
                r#"{{"id":"x","symbol":"x","status":"open","type":"limit","side":"buy","price":1,"amount":1,"filled":0,"remaining":1,"reduceOnly":false,"lastUpdate":0{info}}}"#
            ),
            bits,
            |v| v.info.as_ref(),
        );
        measure::<safe_cex_types::Position>(
            &format!(
                r#"{{"id":"x","symbol":"x","side":"long","entryPrice":1,"notional":1,"leverage":1,"unrealizedPnl":0,"contracts":1,"liquidationPrice":0,"lastUpdate":0{info}}}"#
            ),
            bits,
            |v| v.info.as_ref(),
        );
        measure::<safe_cex_types::Ticker>(
            &format!(
                r#"{{"id":"x","symbol":"x","bid":1,"ask":1,"last":1,"mark":1,"index":1,"percentage":1,"openInterest":1,"fundingRate":1,"fundingTime":1,"volume":1,"quoteVolume":1{info}}}"#
            ),
            bits,
            |v| v.info.as_ref(),
        );
        measure::<safe_cex_types::Balance>(
            &format!(r#"{{"used":0,"free":1,"total":1,"upnl":0{info}}}"#),
            bits,
            |v| v.info.as_ref(),
        );
    }
}

#[test]
fn shared_info_stock_value_negative_control_projects_infinity_to_null_and_guard_refuses() {
    // Negative control deliberately models the old info visitor after tagged
    // Content: native Infinity becomes null. Never repair this fixture to use
    // retained capture; it proves the named passthrough does not waive the guard.
    #[derive(Debug, Deserialize, Serialize)]
    struct WrongInfo {
        info: Option<Value>,
    }
    impl JsNumberOutput for WrongInfo {
        fn check_number_output(&self) {
            safe_cex_num::js_number_output!(self, struct {} passthrough { info });
        }
    }
    let result = js_from_str::<SharedInfoTagged<WrongInfo>>(
        r#"{"kind":"Data","value":{"info":{"nested":[1e400]}}}"#,
    );
    assert!(
        result.is_err(),
        "shared info stock Value projection guard accepted a projection"
    );
    let error = result.unwrap_err();
    assert!(
        error
            .to_string()
            .contains("numeric ingress lost Number at $.value.info.nested[0]"),
        "shared info stock Value projection guard: {error}"
    );
}
