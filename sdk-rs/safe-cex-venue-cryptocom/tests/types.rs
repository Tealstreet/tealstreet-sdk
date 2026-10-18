use safe_cex_num::prototype_realm::PrototypeValue;
use safe_cex_venue_cryptocom::types::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static CORPUS: OnceLock<Result<Value, String>> = OnceLock::new();
    let corpus = CORPUS
        .get_or_init(|| {
            let output = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/tests/types_oracle.cjs"
                ))
                .output()
                .map_err(|error| error.to_string())?;
            if !output.status.success() {
                return Err(String::from_utf8_lossy(&output.stderr).into_owned());
            }
            let corpus: Value =
                serde_json::from_slice(&output.stdout).map_err(|error| error.to_string())?;
            let runtime = &corpus["runtime"];
            if runtime["node"].as_str().is_none()
                || runtime["nodeMajor"].as_u64().is_none()
                || runtime["arch"].as_str().is_none()
                || runtime["platform"].as_str().is_none()
            {
                return Err("oracle_runtime_metadata".into());
            }
            eprintln!(
                "oracle_runtime: {runtime}; rust_arch: {}",
                std::env::consts::ARCH
            );
            Ok(corpus)
        })
        .as_ref()
        .unwrap_or_else(|error| panic!("typescript_oracle: {error}"));
    validate_cardinality(corpus);
    corpus
}

// Independent admission inventory: labels/counts only, no expected native payloads.
// Validate after OnceLock initialization so an assertion cannot restart Node.
fn validate_wire_schema_inventory(reference: &Value) {
    let mut side_literals: Vec<&str> = reference["wire"]["CryptocomTrade"]["fields"]
        .as_array()
        .unwrap()
        .iter()
        .find(|field| field[0] == "s")
        .unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|literal| literal.as_str().unwrap())
        .collect();
    side_literals.sort_unstable();
    assert_eq!(side_literals, ["BUY", "SELL"], "trade_side_literal_census");
    let fill = &reference["wire"]["CryptocomTradeFill"];
    let fields = fill["fields"].as_array().unwrap();
    let optional: Vec<&str> = fields
        .iter()
        .filter(|field| field.get(3) == Some(&Value::Bool(true)))
        .map(|field| field[0].as_str().unwrap())
        .collect();
    assert_eq!(optional, ["client_oid"], "trade_fill_optional_census");
    for (field, expected) in [
        ("journal_type", vec!["TRADING"]),
        ("side", vec!["BUY", "SELL"]),
        ("taker_side", vec!["BUY", "SELL"]),
        ("liquidity_indicator", vec!["MAKER", "TAKER"]),
    ] {
        let mut actual: Vec<&str> = fields.iter().find(|row| row[0] == field).unwrap()[2]
            .as_array()
            .unwrap()
            .iter()
            .map(|value| value.as_str().unwrap())
            .collect();
        actual.sort_unstable();
        assert_eq!(actual, expected, "trade_fill_literal_census:{field}");
    }
}

fn validate_cardinality(corpus: &Value) {
    validate_wire_schema_inventory(corpus);
    validate_order_schema(corpus);
    let inventory: Value = serde_json::from_str(include_str!("case_inventory.json")).unwrap();
    assert_eq!(
        corpus["sourceCardinality"]["summary"],
        json!([0, 1, 2]),
        "source_population:summary"
    );
    let mut book_sizes = vec![
        json!([]),
        json!([[1, 2]]),
        json!([[1, 2], [2, 3]]),
        json!([[0, 0]]),
    ];
    book_sizes.extend((0..21).map(|_| json!([[1, 2], [2, 3]])));
    assert_eq!(
        corpus["sourceCardinality"]["book"],
        json!(book_sizes),
        "source_population:book"
    );
    let order_sizes: Vec<Value> = inventory["wire"]["CryptocomOrder"]
        .as_array()
        .unwrap()
        .iter()
        .map(|label| match label.as_str().unwrap() {
            "absent:exec_inst" => Value::Null,
            "array:exec_inst:0" => json!(0),
            "array:exec_inst:1" => json!(1),
            "array:exec_inst:3" => json!(3),
            _ => json!(2),
        })
        .collect();
    assert_eq!(
        corpus["sourceCardinality"]["order"],
        json!(order_sizes),
        "source_population:order"
    );
    let expected_wire = inventory["wire"].as_object().unwrap();
    assert_eq!(
        corpus["wire"].as_object().unwrap().len(),
        expected_wire.len(),
        "case_inventory:records"
    );
    let mut total = 0;
    for (name, labels) in expected_wire {
        let cases = corpus["wire"][name]["cases"].as_array().unwrap();
        assert_eq!(
            cases.len(),
            labels.as_array().unwrap().len(),
            "source_case_count:{name}"
        );
        let actual: Vec<_> = cases.iter().map(|row| row["label"].clone()).collect();
        assert_eq!(json!(actual), *labels, "case_inventory:{name}");
        for row in cases {
            assert_eq!(
                row["observed"].as_object().unwrap().len(),
                corpus["wire"][name]["fields"].as_array().unwrap().len(),
                "source_field_count:{name}"
            );
        }
        total += cases.len();
    }
    let intervals = corpus["intervals"].as_array().unwrap();
    assert_eq!(intervals.len(), 31, "source_case_count:intervals");
    assert_eq!(
        json!(
            intervals
                .iter()
                .map(|row| row["input"].clone())
                .collect::<Vec<_>>()
        ),
        inventory["intervals"],
        "case_inventory:intervals"
    );
    total += intervals.len();
    for (name, count) in [("accounts", 3), ("summary", 3), ("book", 25)] {
        let cases = corpus[name]["cases"].as_array().unwrap();
        assert_eq!(cases.len(), count, "source_case_count:{name}");
        let distinct: std::collections::BTreeSet<_> = cases
            .iter()
            .map(|row| row["raw"].as_str().unwrap())
            .collect();
        assert_eq!(distinct.len(), count, "case_inventory:distinct:{name}");
        total += cases.len();
    }
    assert_eq!(total, 300, "source_case_count:total");
    let constants = corpus["constants"].as_object().unwrap();
    assert_eq!(constants.len(), 8, "source_case_count:constants");
    for name in inventory["constants"].as_array().unwrap() {
        assert!(
            constants.contains_key(name.as_str().unwrap()),
            "case_inventory:constant:{name}"
        );
    }
    for name in ["summary", "book"] {
        let paths: Vec<_> = corpus[name]["surrogates"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["path"].clone())
            .collect();
        assert_eq!(
            json!(paths),
            inventory["surrogates"][name],
            "case_inventory:surrogates:{name}"
        );
    }
}

fn object(entries: &[(&str, &str)]) -> Value {
    entries
        .iter()
        .map(|(key, value)| (String::from(*key), json!(value)))
        .collect()
}

#[test]
fn live_types_constants() {
    let reference = oracle();
    let ws: Value = WSS_URLS
        .iter()
        .map(|(key, entries)| (String::from(*key), object(entries)))
        .collect();
    let actual = json!({
        "BASE_URLS": object(BASE_URLS),
        "WSS_URLS": ws,
        "CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS": CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS,
        "CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS": CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS,
        "ENDPOINTS": object(ENDPOINTS),
        "PUBLIC_ENDPOINTS": PUBLIC_ENDPOINTS.slots().into_iter().flatten().map(|units| String::from_utf16(&units).unwrap()).collect::<Vec<_>>(),
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
    });
    for (name, expected) in reference["constants"].as_object().unwrap() {
        if name.ends_with("_MS") {
            // Native bits, not number serialization: +0 and -0 cannot converge here.
            let native = if name.contains("RECONNECT") {
                CRYPTOCOM_WS_IDLE_RECONNECT_THRESHOLD_MS
            } else {
                CRYPTOCOM_WS_IDLE_PING_THRESHOLD_MS
            };
            assert_eq!(
                format!("{:016x}", native.to_bits()),
                reference["numberObservations"][name]["bits"]
                    .as_str()
                    .unwrap(),
                "constant_bits:{name}"
            );
        } else {
            assert_eq!(&actual[name], expected, "constant_value:{name}");
        }
    }
    assert_eq!(
        actual.as_object().unwrap().len(),
        reference["constants"].as_object().unwrap().len(),
        "constant_census"
    );
}

#[test]
fn live_candlestick_intervals() {
    let reference = oracle();
    let mut compared = 0;
    for row in reference["intervals"].as_array().unwrap() {
        let input = row["input"].as_str().unwrap();
        let actual = map_candlestick_interval(input);
        if let Some(expected) = row.get("error") {
            assert_eq!(
                actual.unwrap_err(),
                expected.as_str().unwrap(),
                "interval_error:{input}"
            );
            compared += 1;
            continue;
        }
        let (kind, inherited, value) = match actual.unwrap() {
            PrototypeValue::String(value) => ("string", false, value),
            PrototypeValue::Reference(reference) => (
                if reference.is_callable() {
                    "function"
                } else {
                    "object"
                },
                true,
                reference.js_string(),
            ),
            _ => panic!("interval unexpected primitive:{input}"),
        };
        assert_eq!(kind, row["type"], "interval_type:{input}");
        assert_eq!(inherited, row["inherited"], "interval_inherited:{input}");
        assert_eq!(value, row["value"], "interval_value:{input}");
        compared += 1;
    }
    assert_eq!(compared, 31, "comparisons_executed:intervals");
    assert_eq!(
        reference["surrogateErrorUnits"]
            .as_array()
            .unwrap()
            .last()
            .unwrap(),
        0xd800,
        "lone_surrogate_diagnostic_retained"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "rust_utf8_boundary_refuses_lone_surrogate"
    );
}

use safe_cex_num::numeric_ingress::js_from_str;
use safe_cex_venue_cryptocom::wire::*;

fn observe_string(value: &str) -> Value {
    json!({"type":"string", "value":value})
}
fn observe_boolean(value: bool) -> Value {
    json!({"type":"boolean", "value":value})
}
fn observe_number(value: f64) -> Value {
    json!({"type":"number", "bits":format!("{:016x}", if value.is_nan() { 0x7ff8_0000_0000_0000_u64 } else {value.to_bits()})})
}

#[test]
fn live_wire_records() {
    let reference = oracle();
    let mut compared = 0;
    for row in reference["wire"]["CryptocomInstrument"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomInstrument = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomInstrument:{error}"));
        let actual = json!({
            "symbol": observe_string(&value.symbol),
            "inst_type": observe_string(&value.inst_type),
            "display_name": observe_string(&value.display_name),
            "base_ccy": observe_string(&value.base_ccy),
            "quote_ccy": observe_string(&value.quote_ccy),
            "quote_decimals": observe_number(value.quote_decimals),
            "quantity_decimals": observe_number(value.quantity_decimals),
            "price_tick_size": observe_string(&value.price_tick_size),
            "qty_tick_size": observe_string(&value.qty_tick_size),
            "max_leverage": observe_string(&value.max_leverage),
            "tradable": observe_boolean(value.tradable),
            "expiry_timestamp_ms": observe_number(value.expiry_timestamp_ms),
            "beta_product": observe_boolean(value.beta_product),
            "underlying_symbol": observe_string(&value.underlying_symbol),
            "contract_size": observe_string(&value.contract_size),
            "margin_buy_enabled": observe_boolean(value.margin_buy_enabled),
            "margin_sell_enabled": observe_boolean(value.margin_sell_enabled),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomInstrument"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomInstrument:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 26, "comparisons_executed:CryptocomInstrument");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomInstrument"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomInstrument"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomInstrument>(&missing.to_string()).is_err(),
            "required_field:CryptocomInstrument:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomInstrument"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomInstrument"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomInstrument"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomInstrument>(
            reference["wire"]["CryptocomInstrument"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomInstrument"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomInstrument>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomInstrument"
    );
    for field in reference["wire"]["CryptocomInstrument"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomInstrument>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomInstrument:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomInstrument"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomInstrument"
    );
    let extras: CryptocomInstrument = js_from_str(
        reference["wire"]["CryptocomInstrument"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomInstrument"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomInstrument"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomInstrument>(raw).is_err(),
            "wire_shape_refusal:CryptocomInstrument:{raw}"
        );
    }
    let mut compared = 0;
    for row in reference["wire"]["CryptocomTicker"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomTicker = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomTicker:{error}"));
        let actual = json!({
            "i": observe_string(&value.i),
            "b": observe_string(&value.b),
            "k": observe_string(&value.k),
            "a": observe_string(&value.a),
            "c": observe_string(&value.c),
            "h": observe_string(&value.h),
            "l": observe_string(&value.l),
            "v": observe_string(&value.v),
            "vv": observe_string(&value.vv),
            "oi": observe_string(&value.oi),
            "t": observe_number(value.t),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomTicker"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomTicker:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 8, "comparisons_executed:CryptocomTicker");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomTicker"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomTicker"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomTicker>(&missing.to_string()).is_err(),
            "required_field:CryptocomTicker:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomTicker"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomTicker"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomTicker"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomTicker>(
            reference["wire"]["CryptocomTicker"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomTicker"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomTicker>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomTicker"
    );
    for field in reference["wire"]["CryptocomTicker"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomTicker>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomTicker:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomTicker"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomTicker"
    );
    let extras: CryptocomTicker = js_from_str(
        reference["wire"]["CryptocomTicker"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomTicker"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomTicker"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomTicker>(raw).is_err(),
            "wire_shape_refusal:CryptocomTicker:{raw}"
        );
    }
    let mut compared = 0;
    for row in reference["wire"]["CryptocomCandlestick"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomCandlestick = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomCandlestick:{error}"));
        let actual = json!({
            "t": observe_number(value.t),
            "o": observe_string(&value.o),
            "h": observe_string(&value.h),
            "l": observe_string(&value.l),
            "c": observe_string(&value.c),
            "v": observe_string(&value.v),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomCandlestick"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomCandlestick:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 8, "comparisons_executed:CryptocomCandlestick");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomCandlestick"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomCandlestick"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomCandlestick>(&missing.to_string()).is_err(),
            "required_field:CryptocomCandlestick:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomCandlestick"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomCandlestick"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomCandlestick"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomCandlestick>(
            reference["wire"]["CryptocomCandlestick"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomCandlestick"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomCandlestick>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomCandlestick"
    );
    for field in reference["wire"]["CryptocomCandlestick"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomCandlestick>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomCandlestick:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomCandlestick"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomCandlestick"
    );
    let extras: CryptocomCandlestick = js_from_str(
        reference["wire"]["CryptocomCandlestick"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomCandlestick"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomCandlestick"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomCandlestick>(raw).is_err(),
            "wire_shape_refusal:CryptocomCandlestick:{raw}"
        );
    }
    let mut compared = 0;
    for row in reference["wire"]["CryptocomPositionBalance"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomPositionBalance = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomPositionBalance:{error}"));
        let actual = json!({
            "instrument_name": observe_string(&value.instrument_name),
            "quantity": observe_string(&value.quantity),
            "reserved_qty": observe_string(&value.reserved_qty),
            "collateral_amount": observe_string(&value.collateral_amount),
            "collateral_eligible": observe_boolean(value.collateral_eligible),
            "haircut": observe_string(&value.haircut),
            "market_value": observe_string(&value.market_value),
            "max_withdrawal_balance": observe_string(&value.max_withdrawal_balance),
            "hourly_interest_rate": observe_string(&value.hourly_interest_rate),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomPositionBalance"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomPositionBalance:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 2, "comparisons_executed:CryptocomPositionBalance");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomPositionBalance"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomPositionBalance"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomPositionBalance>(&missing.to_string()).is_err(),
            "required_field:CryptocomPositionBalance:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomPositionBalance"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomPositionBalance"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomPositionBalance"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomPositionBalance>(
            reference["wire"]["CryptocomPositionBalance"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomPositionBalance"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomPositionBalance>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomPositionBalance"
    );
    for field in reference["wire"]["CryptocomPositionBalance"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomPositionBalance>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomPositionBalance:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomPositionBalance"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomPositionBalance"
    );
    let extras: CryptocomPositionBalance = js_from_str(
        reference["wire"]["CryptocomPositionBalance"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomPositionBalance"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomPositionBalance"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomPositionBalance>(raw).is_err(),
            "wire_shape_refusal:CryptocomPositionBalance:{raw}"
        );
    }
    let mut compared = 0;
    for row in reference["wire"]["CryptocomError"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomError = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomError:{error}"));
        let actual = json!({
            "code": observe_number(value.code),
            "message": observe_string(&value.message),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomError"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomError:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 8, "comparisons_executed:CryptocomError");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomError"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomError"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomError>(&missing.to_string()).is_err(),
            "required_field:CryptocomError:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomError"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomError"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomError"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomError>(
            reference["wire"]["CryptocomError"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomError"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomError>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomError"
    );
    for field in reference["wire"]["CryptocomError"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomError>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomError:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomError"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomError"
    );
    let extras: CryptocomError = js_from_str(
        reference["wire"]["CryptocomError"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomError"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomError"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomError>(raw).is_err(),
            "wire_shape_refusal:CryptocomError:{raw}"
        );
    }
    let mut compared = 0;
    for row in reference["wire"]["CryptocomTrade"]["cases"]
        .as_array()
        .unwrap()
    {
        let value: CryptocomTrade = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomTrade:{error}"));
        let actual = json!({
            "d": observe_number(value.d),
            "t": observe_number(value.t),
            "p": observe_number(value.p),
            "q": observe_number(value.q),
            "s": observe_string(match value.s {
                CryptocomTradeSide::Buy => "BUY",
                CryptocomTradeSide::Sell => "SELL",
            }),
            "i": observe_string(&value.i),
            "dataTime": observe_number(value.data_time),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "wire_field_census:CryptocomTrade"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomTrade:{field}:{}",
                row["label"]
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 38, "comparisons_executed:CryptocomTrade");
    let base: Value = serde_json::from_str(
        reference["wire"]["CryptocomTrade"]["cases"][0]["raw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    for field in reference["wire"]["CryptocomTrade"]["fields"]
        .as_array()
        .unwrap()
    {
        let field = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomTrade>(&missing.to_string()).is_err(),
            "required_field:CryptocomTrade:{field}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomTrade"]["surrogateUnits"],
        json!([0xd800]),
        "wire_lone_surrogate:CryptocomTrade"
    );
    typed_utf16::assert_projection(
        reference["wire"]["CryptocomTrade"]["surrogateRaw"]
            .as_str()
            .unwrap(),
        &(js_from_str::<CryptocomTrade>(
            reference["wire"]["CryptocomTrade"]["surrogateRaw"]
                .as_str()
                .unwrap(),
        ))
        .expect("typed surrogate record is retained"),
    );
    let positional = Value::Array(
        reference["wire"]["CryptocomTrade"]["fields"]
            .as_array()
            .unwrap()
            .iter()
            .map(|field| base[field[0].as_str().unwrap()].clone())
            .collect(),
    );
    assert!(
        js_from_str::<CryptocomTrade>(&positional.to_string()).is_err(),
        "populated_array_refusal:CryptocomTrade"
    );
    for field in reference["wire"]["CryptocomTrade"]["fields"]
        .as_array()
        .unwrap()
    {
        let field_name = field[0].as_str().unwrap();
        let mut wrong = base.clone();
        wrong[field_name] = if field[1] == "number" {
            json!("37.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomTrade>(&wrong.to_string()).is_err(),
            "scalar_kind_refusal:CryptocomTrade:{field_name}"
        );
    }
    assert_eq!(
        reference["wire"]["CryptocomTrade"]["extrasRetained"], true,
        "ts_extra_retained:CryptocomTrade"
    );
    let extras: CryptocomTrade = js_from_str(
        reference["wire"]["CryptocomTrade"]["extrasRaw"]
            .as_str()
            .unwrap(),
    )
    .unwrap();
    assert_eq!(
        extras.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomTrade"
    );
    assert!(
        serde_json::to_value(extras)
            .unwrap()
            .get("__extra")
            .is_some(),
        "wire_extra_retained:CryptocomTrade"
    );
    for raw in ["[]", "null", "true", "17", "{}"] {
        assert!(
            js_from_str::<CryptocomTrade>(raw).is_err(),
            "wire_shape_refusal:CryptocomTrade:{raw}"
        );
    }
    for invalid in ["buy", "sell", "", "HOLD"] {
        let mut wrong = base.clone();
        wrong["s"] = json!(invalid);
        assert!(
            js_from_str::<CryptocomTrade>(&wrong.to_string()).is_err(),
            "trade_side_literal_refusal:{invalid}"
        );
    }
    for invalid in [
        json!({"BUY": null}),
        json!({"SELL": null}),
        json!(["BUY"]),
        json!(null),
        json!(true),
    ] {
        let mut wrong = base.clone();
        wrong["s"] = invalid;
        assert!(
            js_from_str::<CryptocomTrade>(&wrong.to_string()).is_err(),
            "trade_side_string_shape_refusal:{}",
            wrong["s"]
        );
    }
    check_account_response(reference);
    check_balance_summary(reference);
    check_book(reference);
    check_trade_fill(reference);
    check_order(reference);
}

fn check_account_response(reference: &Value) {
    let accounts = &reference["accounts"];
    assert_eq!(
        accounts["fields"],
        json!([["this_account", "object", [["uuid", "string"]]]]),
        "account_native_field_census"
    );
    let mut compared = 0;
    for row in accounts["cases"].as_array().unwrap() {
        let value: CryptocomAccountsResponse = js_from_str(row["raw"].as_str().unwrap()).unwrap();
        assert_eq!(
            observe_string(&value.this_account.uuid),
            row["uuid"],
            "decoded_field:CryptocomAccountsResponse:this_account.uuid"
        );
        compared += 1;
    }
    assert_eq!(compared, 3, "comparisons_executed:accounts");
    for raw in [
        r#"{}"#,
        r#"{"this_account":{}}"#,
        r#"{"this_account":null}"#,
        r#"{"this_account":{"uuid":null}}"#,
        r#"{"this_account":{"uuid":42}}"#,
        r#"{"this_account":{"uuid":true}}"#,
        r#"{"this_account":{"uuid":[]}}"#,
        r#"{"this_account":{"uuid":{}}}"#,
        r#"{"this_account":true}"#,
        r#"{"this_account":17}"#,
        r#"{"this_account":"uuid"}"#,
        "null",
    ] {
        assert!(
            js_from_str::<CryptocomAccountsResponse>(raw).is_err(),
            "account_shape_refusal:{raw}"
        );
    }
    for (label, raw) in [
        ("outer", r#"[{"uuid":"positional"}]"#),
        ("inner", r#"{"this_account":["positional"]}"#),
    ] {
        assert!(
            js_from_str::<CryptocomAccountsResponse>(raw).is_err(),
            "account_populated_array_refusal:{label}"
        );
    }
    assert_eq!(
        accounts["surrogateUnits"],
        json!([0xd800]),
        "account_ts_surrogate_retained"
    );
    typed_utf16::assert_projection(
        accounts["surrogateRaw"].as_str().unwrap(),
        &(js_from_str::<CryptocomAccountsResponse>(accounts["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    assert_eq!(
        accounts["extrasObserved"],
        json!(["outer", "inner"]),
        "account_ts_nested_extras_retained"
    );
    let extras: CryptocomAccountsResponse =
        js_from_str(accounts["extrasRaw"].as_str().unwrap()).unwrap();
    assert_eq!(
        extras.this_account.uuid, "nested-extra",
        "account_extras_native_uuid"
    );
    assert_eq!(
        extras.additional_fields["__outer"].as_str(),
        Some("outer"),
        "native_account_outer_extra"
    );
    assert_eq!(
        extras.this_account.additional_fields["__inner"].as_str(),
        Some("inner"),
        "native_account_inner_extra"
    );
    let encoded = serde_json::to_value(extras).unwrap();
    assert!(
        encoded.get("__outer").is_some(),
        "account_wire_outer_extra_retained"
    );
    assert!(
        encoded["this_account"].get("__inner").is_some(),
        "account_wire_inner_extra_retained"
    );
}

fn check_balance_summary(reference: &Value) {
    let summary = &reference["summary"];
    let mut compared = 0;
    for (case_index, row) in summary["cases"].as_array().unwrap().iter().enumerate() {
        let value: CryptocomBalanceSummary = js_from_str(row["raw"].as_str().unwrap()).unwrap();
        let expected_count = usize::try_from(
            reference["sourceCardinality"]["summary"][case_index]
                .as_u64()
                .unwrap(),
        )
        .unwrap();
        assert_eq!(
            value.position_balances.len(),
            expected_count,
            "expected_native_count:summary"
        );
        assert_eq!(
            row["observed"]["position_balances"]
                .as_array()
                .unwrap()
                .len(),
            expected_count,
            "projected_count:summary"
        );
        let positions: Vec<Value> = value
            .position_balances
            .iter()
            .map(|position| {
                json!({
                    "instrument_name": observe_string(&position.instrument_name),
                    "quantity": observe_string(&position.quantity),
                    "reserved_qty": observe_string(&position.reserved_qty),
                    "collateral_amount": observe_string(&position.collateral_amount),
                    "collateral_eligible": observe_boolean(position.collateral_eligible),
                    "haircut": observe_string(&position.haircut),
                    "market_value": observe_string(&position.market_value),
                    "max_withdrawal_balance": observe_string(&position.max_withdrawal_balance),
                    "hourly_interest_rate": observe_string(&position.hourly_interest_rate),
                })
            })
            .collect();
        let actual = json!({
            "total_available_balance": observe_string(&value.total_available_balance),
            "total_margin_balance": observe_string(&value.total_margin_balance),
            "total_initial_margin": observe_string(&value.total_initial_margin),
            "total_maintenance_margin": observe_string(&value.total_maintenance_margin),
            "total_cash_balance": observe_string(&value.total_cash_balance),
            "total_collateral_value": observe_string(&value.total_collateral_value),
            "total_session_unrealized_pnl": observe_string(&value.total_session_unrealized_pnl),
            "total_session_realized_pnl": observe_string(&value.total_session_realized_pnl),
            "total_position_cost": observe_string(&value.total_position_cost),
            "instrument_name": observe_string(&value.instrument_name),
            "position_balances": positions,
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "summary_field_census"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomBalanceSummary:{field}"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 3, "comparisons_executed:summary");
    let base: Value = serde_json::from_str(summary["cases"][2]["raw"].as_str().unwrap()).unwrap();
    for field in summary["fields"].as_array().unwrap() {
        let name = field[0].as_str().unwrap();
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert!(
            js_from_str::<CryptocomBalanceSummary>(&missing.to_string()).is_err(),
            "summary_required_field:{name}"
        );
        for invalid in [Value::Null, json!(42), json!(false), json!({})] {
            let mut wrong = base.clone();
            wrong[name] = invalid;
            assert!(
                js_from_str::<CryptocomBalanceSummary>(&wrong.to_string()).is_err(),
                "summary_scalar_kind_refusal:{name}"
            );
        }
    }
    let positional: Vec<Value> = summary["fields"]
        .as_array()
        .unwrap()
        .iter()
        .map(|field| base[field[0].as_str().unwrap()].clone())
        .collect();
    assert!(
        js_from_str::<CryptocomBalanceSummary>(&serde_json::to_string(&positional).unwrap())
            .is_err(),
        "summary_populated_array_refusal"
    );
    for invalid in [json!(null), json!(17), json!("item"), json!(["positional"])] {
        let mut wrong = base.clone();
        wrong["position_balances"][1] = invalid;
        assert!(
            js_from_str::<CryptocomBalanceSummary>(&wrong.to_string()).is_err(),
            "summary_nested_shape_refusal"
        );
    }
    for row in summary["surrogates"].as_array().unwrap() {
        assert_eq!(
            row["units"],
            json!([0xd800]),
            "summary_ts_surrogate_retained"
        );
        typed_utf16::assert_projection(
            row["raw"].as_str().unwrap(),
            &(js_from_str::<CryptocomBalanceSummary>(row["raw"].as_str().unwrap()))
                .expect("typed surrogate record is retained"),
        );
    }
    assert_eq!(
        summary["extrasObserved"],
        json!(["outer", "inner:0", "inner:1"]),
        "summary_ts_extras_retained"
    );
    let extras: CryptocomBalanceSummary =
        js_from_str(summary["extrasRaw"].as_str().unwrap()).unwrap();
    assert_eq!(
        extras.position_balances.len(),
        2,
        "expected_native_count:summary:extras"
    );
    assert_eq!(
        extras.additional_fields["__outer"].as_str(),
        Some("outer"),
        "native_summary_outer_extra"
    );
    for (index, child) in extras.position_balances.iter().enumerate() {
        assert_eq!(
            child.additional_fields["__inner"].as_str(),
            Some(format!("inner:{index}").as_str()),
            "native_summary_inner_extra"
        );
    }
    let encoded = serde_json::to_value(extras).unwrap();
    assert_eq!(
        encoded["position_balances"].as_array().unwrap().len(),
        2,
        "encoded_count:summary:extras"
    );
    assert!(
        encoded.get("__outer").is_some(),
        "summary_wire_outer_extra_retained"
    );
    for row in encoded["position_balances"].as_array().unwrap() {
        assert!(
            row.get("__inner").is_some(),
            "summary_wire_inner_extra_retained"
        );
    }
}

fn check_book(reference: &Value) {
    let book = &reference["book"];
    let mut compared = 0;
    for (case_index, row) in book["cases"].as_array().unwrap().iter().enumerate() {
        let value: CryptocomBook = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomBook:{error}"));
        let expected = reference["sourceCardinality"]["book"][case_index]
            .as_array()
            .unwrap();
        assert_eq!(
            value.data.len(),
            expected.len(),
            "expected_native_count:book"
        );
        assert_eq!(
            row["observed"]["data"].as_array().unwrap().len(),
            expected.len(),
            "projected_count:book"
        );
        for (index, native) in value.data.iter().enumerate() {
            let bid_count = usize::try_from(expected[index][0].as_u64().unwrap()).unwrap();
            let ask_count = usize::try_from(expected[index][1].as_u64().unwrap()).unwrap();
            assert_eq!(
                native.bids.len(),
                bid_count,
                "expected_native_count:book:bids"
            );
            assert_eq!(
                native.asks.len(),
                ask_count,
                "expected_native_count:book:asks"
            );
            assert_eq!(
                row["observed"]["data"][index]["bids"]
                    .as_array()
                    .unwrap()
                    .len(),
                bid_count,
                "projected_count:book:bids"
            );
            assert_eq!(
                row["observed"]["data"][index]["asks"]
                    .as_array()
                    .unwrap()
                    .len(),
                ask_count,
                "projected_count:book:asks"
            );
        }
        let data: Vec<Value> = value
            .data
            .iter()
            .map(|row| {
                let bids: Vec<_> = row
                    .bids
                    .iter()
                    .map(|tuple| tuple.each_ref().map(|value| observe_string(value)))
                    .collect();
                let asks: Vec<_> = row
                    .asks
                    .iter()
                    .map(|tuple| tuple.each_ref().map(|value| observe_string(value)))
                    .collect();
                json!({"bids":bids, "asks":asks, "t":observe_number(row.t)})
            })
            .collect();
        let actual = json!({"instrument_name":observe_string(&value.instrument_name), "depth":observe_number(value.depth), "data":data});
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "book_native_field_census"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomBook:{field}"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 25, "comparisons_executed:book");
    let base: Value = serde_json::from_str(book["cases"][2]["raw"].as_str().unwrap()).unwrap();
    for field in ["instrument_name", "depth", "data"] {
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomBook>(&missing.to_string()).is_err(),
            "book_required_field:{field}"
        );
    }
    for field in ["bids", "asks", "t"] {
        let mut missing = base.clone();
        missing["data"][1].as_object_mut().unwrap().remove(field);
        assert!(
            js_from_str::<CryptocomBook>(&missing.to_string()).is_err(),
            "book_nested_required_field:{field}"
        );
    }
    for (label, value) in [
        (
            "outer",
            json!([base["instrument_name"], base["depth"], base["data"]]),
        ),
        ("inner", {
            let mut value = base.clone();
            value["data"][1] = json!([
                base["data"][1]["bids"],
                base["data"][1]["asks"],
                base["data"][1]["t"]
            ]);
            value
        }),
    ] {
        assert!(
            js_from_str::<CryptocomBook>(&value.to_string()).is_err(),
            "book_populated_array_refusal:{label}"
        );
    }
    for side in ["bids", "asks"] {
        for invalid in [
            json!([]),
            json!(["price", "qty"]),
            json!(["price", "qty", "count", "extra"]),
            json!(["price", 2, "count"]),
            json!(["price", "qty", null]),
            json!({"0":"price","1":"qty","2":"count"}),
        ] {
            let mut wrong = base.clone();
            wrong["data"][1][side][0] = invalid;
            assert!(
                js_from_str::<CryptocomBook>(&wrong.to_string()).is_err(),
                "book_tuple_refusal:{side}"
            );
        }
        for invalid in [json!(null), json!({}), json!("side")] {
            let mut wrong = base.clone();
            wrong["data"][1][side] = invalid;
            assert!(
                js_from_str::<CryptocomBook>(&wrong.to_string()).is_err(),
                "book_side_array_refusal:{side}"
            );
        }
    }
    for (field, invalid) in [
        ("depth", json!("7")),
        ("depth", json!(null)),
        ("instrument_name", json!(17)),
        ("data", json!({})),
        ("data", json!(null)),
    ] {
        let mut wrong = base.clone();
        wrong[field] = invalid;
        assert!(
            js_from_str::<CryptocomBook>(&wrong.to_string()).is_err(),
            "book_scalar_kind_refusal:{field}"
        );
    }
    for invalid in [json!("1001"), json!(null), json!(false)] {
        let mut wrong = base.clone();
        wrong["data"][1]["t"] = invalid;
        assert!(
            js_from_str::<CryptocomBook>(&wrong.to_string()).is_err(),
            "book_timestamp_kind_refusal"
        );
    }
    for row in book["surrogates"].as_array().unwrap() {
        assert_eq!(row["units"], json!([0xd800]), "book_ts_surrogate_retained");
        typed_utf16::assert_projection(
            row["raw"].as_str().unwrap(),
            &(js_from_str::<CryptocomBook>(row["raw"].as_str().unwrap()))
                .expect("typed surrogate record is retained"),
        );
    }
    assert_eq!(
        book["extrasObserved"],
        json!(["outer", "inner"]),
        "book_ts_extras_retained"
    );
    let extras: CryptocomBook = js_from_str(book["extrasRaw"].as_str().unwrap()).unwrap();
    assert_eq!(extras.data.len(), 2, "expected_native_count:book:extras");
    assert_eq!(
        extras.additional_fields["__outer"].as_str(),
        Some("outer"),
        "native_book_outer_extra"
    );
    assert_eq!(
        extras.data[1].additional_fields["__inner"].as_str(),
        Some("inner"),
        "native_book_inner_extra"
    );
    let encoded = serde_json::to_value(extras).unwrap();
    assert_eq!(
        encoded["data"].as_array().unwrap().len(),
        2,
        "encoded_count:book:extras"
    );
    assert!(
        encoded.get("__outer").is_some(),
        "book_wire_outer_extra_retained"
    );
    assert!(
        encoded["data"][1].get("__inner").is_some(),
        "book_wire_inner_extra_retained"
    );
}

fn check_trade_fill(reference: &Value) {
    let fill = &reference["wire"]["CryptocomTradeFill"];
    let fields = fill["fields"].as_array().unwrap();
    let mut compared = 0;
    for row in fill["cases"].as_array().unwrap() {
        let value: CryptocomTradeFill = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomTradeFill:{error}"));
        let actual = json!({
            "account_id": observe_string(&value.account_id),
            "event_date": observe_number(value.event_date),
            "journal_type": observe_string(match value.journal_type { CryptocomJournalType::Trading => "TRADING" }),
            "traded_quantity": observe_number(value.traded_quantity),
            "traded_price": observe_number(value.traded_price),
            "transaction_qty": observe_number(value.transaction_qty),
            "transaction_cost": observe_number(value.transaction_cost),
            "order_id": observe_string(&value.order_id),
            "trade_id": observe_string(&value.trade_id),
            "trade_match_id": observe_string(&value.trade_match_id),
            "client_oid": value.client_oid.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "taker_side": observe_string(match value.taker_side { CryptocomTradeSide::Buy => "BUY", CryptocomTradeSide::Sell => "SELL" }),
            "side": observe_string(match value.side { CryptocomTradeSide::Buy => "BUY", CryptocomTradeSide::Sell => "SELL" }),
            "instrument_name": observe_string(&value.instrument_name),
            "fee": observe_number(value.fee),
            "fee_currency": observe_string(&value.fee_currency),
            "create_time": observe_number(value.create_time),
            "create_time_ns": observe_string(&value.create_time_ns),
            "liquidity_indicator": observe_string(match value.liquidity_indicator { CryptocomLiquidityIndicator::Maker => "MAKER", CryptocomLiquidityIndicator::Taker => "TAKER" }),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            row["observed"].as_object().unwrap().len(),
            "trade_fill_field_census"
        );
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomTradeFill:{field}:{}",
                row["label"]
            );
        }
        if value.client_oid.is_none() {
            assert!(
                serde_json::to_value(value)
                    .unwrap()
                    .get("client_oid")
                    .is_none(),
                "trade_fill_absent_output_omission"
            );
        }
        compared += 1;
    }
    assert_eq!(compared, 59, "comparisons_executed:fill");
    let base: Value = serde_json::from_str(fill["cases"][0]["raw"].as_str().unwrap()).unwrap();
    for field in fields {
        let name = field[0].as_str().unwrap();
        if name != "client_oid" {
            let mut missing = base.clone();
            missing.as_object_mut().unwrap().remove(name);
            assert!(
                js_from_str::<CryptocomTradeFill>(&missing.to_string()).is_err(),
                "trade_fill_required_field:{name}"
            );
        }
        let mut wrong = base.clone();
        wrong[name] = if field[1] == "number" {
            json!("19.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomTradeFill>(&wrong.to_string()).is_err(),
            "trade_fill_scalar_kind_refusal:{name}"
        );
    }
    for invalid in [json!(null), json!(42), json!([]), json!({}), json!(true)] {
        let mut wrong = base.clone();
        wrong["client_oid"] = invalid;
        assert!(
            js_from_str::<CryptocomTradeFill>(&wrong.to_string()).is_err(),
            "trade_fill_optional_kind_refusal"
        );
    }
    for (field, literal) in [
        ("journal_type", "TRADING"),
        ("side", "BUY"),
        ("taker_side", "SELL"),
        ("liquidity_indicator", "MAKER"),
    ] {
        for invalid in [
            json!("wrong"),
            json!(literal.to_lowercase()),
            json!({(literal):null}),
            json!([literal]),
            json!(null),
        ] {
            let mut wrong = base.clone();
            wrong[field] = invalid;
            assert!(
                js_from_str::<CryptocomTradeFill>(&wrong.to_string()).is_err(),
                "trade_fill_literal_shape_refusal:{field}"
            );
        }
    }
    let positional: Vec<Value> = fields
        .iter()
        .map(|field| base[field[0].as_str().unwrap()].clone())
        .collect();
    assert!(
        js_from_str::<CryptocomTradeFill>(&serde_json::to_string(&positional).unwrap()).is_err(),
        "trade_fill_populated_array_refusal"
    );
    assert_eq!(
        fill["surrogateUnits"],
        json!([0xd800]),
        "trade_fill_ts_surrogate_retained"
    );
    typed_utf16::assert_projection(
        fill["surrogateRaw"].as_str().unwrap(),
        &(js_from_str::<CryptocomTradeFill>(fill["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    assert_eq!(fill["extrasRetained"], true, "trade_fill_ts_extra_retained");
    let extra: CryptocomTradeFill = js_from_str(fill["extrasRaw"].as_str().unwrap()).unwrap();
    assert_eq!(
        extra.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomTradeFill"
    );
    assert!(
        serde_json::to_value(extra)
            .unwrap()
            .get("__extra")
            .is_some(),
        "trade_fill_wire_extra_retained"
    );
}

fn validate_order_schema(reference: &Value) {
    let fields = reference["wire"]["CryptocomOrder"]["fields"]
        .as_array()
        .unwrap();
    let optional: Vec<_> = fields
        .iter()
        .filter(|row| row.get(3) == Some(&Value::Bool(true)))
        .map(|row| row[0].as_str().unwrap())
        .collect();
    assert_eq!(
        optional,
        [
            "account_id",
            "client_oid",
            "exec_inst",
            "limit_price",
            "trigger_price",
            "ref_price",
            "ref_price_type",
            "avg_price",
            "cumulative_quantity",
            "cumulative_value",
            "fee_currency",
            "fee_instrument_name",
            "create_time",
            "update_time",
            "time_created",
            "time_updated",
            "order_status",
            "post_only"
        ],
        "order_optional_census"
    );
    let mut actual: Vec<_> = fields.iter().find(|row| row[0] == "order_type").unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            "LIMIT",
            "MARKET",
            "STOP_LIMIT",
            "STOP_LOSS",
            "TAKE_PROFIT",
            "TAKE_PROFIT_LIMIT"
        ],
        "order_literal_census:order_type"
    );
    let mut actual: Vec<_> = fields.iter().find(|row| row[0] == "time_in_force").unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            "FILL_OR_KILL",
            "GOOD_TILL_CANCEL",
            "IMMEDIATE_OR_CANCEL",
            "POST_ONLY"
        ],
        "order_literal_census:time_in_force"
    );
    let mut actual: Vec<_> = fields.iter().find(|row| row[0] == "side").unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(actual, ["BUY", "SELL"], "order_literal_census:side");
    let mut actual: Vec<_> = fields
        .iter()
        .find(|row| row[0] == "ref_price_type")
        .unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        ["INDEX_PRICE", "LAST_PRICE", "MARK_PRICE"],
        "order_literal_census:ref_price_type"
    );
    let mut actual: Vec<_> = fields.iter().find(|row| row[0] == "status").unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            "ACTIVE", "CANCELED", "EXPIRED", "FILLED", "PENDING", "REJECTED"
        ],
        "order_literal_census:status"
    );
    let mut actual: Vec<_> = fields.iter().find(|row| row[0] == "order_status").unwrap()[2]
        .as_array()
        .unwrap()
        .iter()
        .map(|token| token.as_str().unwrap())
        .collect();
    actual.sort_unstable();
    assert_eq!(
        actual,
        [
            "ACTIVE", "CANCELED", "EXPIRED", "FILLED", "PENDING", "REJECTED"
        ],
        "order_literal_census:order_status"
    );
    assert_eq!(
        fields.iter().find(|row| row[0] == "exec_inst").unwrap()[1],
        "string-array",
        "order_array_census"
    );
}

fn check_order(reference: &Value) {
    let order = &reference["wire"]["CryptocomOrder"];
    let fields = order["fields"].as_array().unwrap();
    let mut compared = 0;
    for (case_index, row) in order["cases"].as_array().unwrap().iter().enumerate() {
        let value: CryptocomOrder = js_from_str(row["raw"].as_str().unwrap())
            .unwrap_or_else(|error| panic!("wire_ingress:CryptocomOrder:{error}"));
        let expected_count = reference["sourceCardinality"]["order"][case_index]
            .as_u64()
            .map(|count| usize::try_from(count).unwrap());
        assert_eq!(
            value.exec_inst.as_ref().map(Vec::len),
            expected_count,
            "expected_native_count:order:exec_inst"
        );
        assert_eq!(
            row["observed"]["exec_inst"]["value"]
                .as_array()
                .map(Vec::len),
            expected_count,
            "projected_count:order:exec_inst"
        );
        let actual = json!({
            "account_id": value.account_id.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "order_id": observe_string(&value.order_id),
            "client_oid": value.client_oid.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "order_type": observe_string(match value.order_type { CryptocomOrderType::Limit => "LIMIT", CryptocomOrderType::Market => "MARKET", CryptocomOrderType::StopLoss => "STOP_LOSS", CryptocomOrderType::StopLimit => "STOP_LIMIT", CryptocomOrderType::TakeProfit => "TAKE_PROFIT", CryptocomOrderType::TakeProfitLimit => "TAKE_PROFIT_LIMIT" }),
            "time_in_force": observe_string(match value.time_in_force { CryptocomTimeInForce::GoodTillCancel => "GOOD_TILL_CANCEL", CryptocomTimeInForce::FillOrKill => "FILL_OR_KILL", CryptocomTimeInForce::ImmediateOrCancel => "IMMEDIATE_OR_CANCEL", CryptocomTimeInForce::PostOnly => "POST_ONLY" }),
            "side": observe_string(match value.side { CryptocomTradeSide::Buy => "BUY", CryptocomTradeSide::Sell => "SELL" }),
            "exec_inst": value.exec_inst.as_deref().map(|native| json!({"type":"object","value":native})).unwrap_or_else(|| json!({"type":"undefined"})),
            "quantity": observe_string(&value.quantity),
            "limit_price": value.limit_price.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "trigger_price": value.trigger_price.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "ref_price": value.ref_price.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "ref_price_type": value.ref_price_type.map(|native| observe_string(match native { CryptocomRefPriceType::Mark => "MARK_PRICE", CryptocomRefPriceType::Index => "INDEX_PRICE", CryptocomRefPriceType::Last => "LAST_PRICE" })).unwrap_or_else(|| json!({"type":"undefined"})),
            "avg_price": value.avg_price.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "cumulative_quantity": value.cumulative_quantity.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "cumulative_value": value.cumulative_value.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "fee_currency": value.fee_currency.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "fee_instrument_name": value.fee_instrument_name.as_deref().map(observe_string).unwrap_or_else(|| json!({"type":"undefined"})),
            "create_time": value.create_time.map(observe_number).unwrap_or_else(|| json!({"type":"undefined"})),
            "update_time": value.update_time.map(observe_number).unwrap_or_else(|| json!({"type":"undefined"})),
            "time_created": value.time_created.map(observe_number).unwrap_or_else(|| json!({"type":"undefined"})),
            "time_updated": value.time_updated.map(observe_number).unwrap_or_else(|| json!({"type":"undefined"})),
            "status": observe_string(match value.status { CryptocomOrderStatus::Active => "ACTIVE", CryptocomOrderStatus::Filled => "FILLED", CryptocomOrderStatus::Canceled => "CANCELED", CryptocomOrderStatus::Expired => "EXPIRED", CryptocomOrderStatus::Pending => "PENDING", CryptocomOrderStatus::Rejected => "REJECTED" }),
            "order_status": value.order_status.map(|native| observe_string(match native { CryptocomOrderStatus::Active => "ACTIVE", CryptocomOrderStatus::Filled => "FILLED", CryptocomOrderStatus::Canceled => "CANCELED", CryptocomOrderStatus::Expired => "EXPIRED", CryptocomOrderStatus::Pending => "PENDING", CryptocomOrderStatus::Rejected => "REJECTED" })).unwrap_or_else(|| json!({"type":"undefined"})),
            "instrument_name": observe_string(&value.instrument_name),
            "post_only": value.post_only.map(observe_boolean).unwrap_or_else(|| json!({"type":"undefined"})),
        });
        assert_eq!(
            actual.as_object().unwrap().len(),
            25,
            "order_native_field_count"
        );
        assert_eq!(
            row["observed"].as_object().unwrap().len(),
            25,
            "order_source_field_count"
        );
        let mut compared_fields = 0;
        for (field, expected) in row["observed"].as_object().unwrap() {
            assert_eq!(
                &actual[field], expected,
                "decoded_field:CryptocomOrder:{field}:{}",
                row["label"]
            );
            compared_fields += 1;
        }
        assert_eq!(compared_fields, 25, "comparisons_executed:order:fields");
        let encoded = serde_json::to_value(&value).unwrap();
        for field in fields
            .iter()
            .filter(|field| field.get(3) == Some(&Value::Bool(true)))
        {
            let name = field[0].as_str().unwrap();
            if row["observed"][name]["type"] == "undefined" {
                assert!(
                    encoded.get(name).is_none(),
                    "order_absent_output_omission:{name}"
                );
            }
        }
        compared += 1;
    }
    assert_eq!(compared, 89, "comparisons_executed:order");
    let base: Value = serde_json::from_str(order["cases"][0]["raw"].as_str().unwrap()).unwrap();
    for field in fields {
        let name = field[0].as_str().unwrap();
        let optional = field.get(3) == Some(&Value::Bool(true));
        let mut missing = base.clone();
        missing.as_object_mut().unwrap().remove(name);
        assert_eq!(
            js_from_str::<CryptocomOrder>(&missing.to_string()).is_ok(),
            optional,
            "order_presence:{name}"
        );
        let mut wrong = base.clone();
        wrong[name] = Value::Null;
        assert!(
            js_from_str::<CryptocomOrder>(&wrong.to_string()).is_err(),
            "order_null_refusal:{name}"
        );
        wrong[name] = if field[1] == "number" {
            json!("19.25")
        } else {
            json!(42)
        };
        assert!(
            js_from_str::<CryptocomOrder>(&wrong.to_string()).is_err(),
            "order_scalar_kind_refusal:{name}"
        );
        if let Some(tokens) = field.get(2).and_then(Value::as_array) {
            let literal = tokens[0].as_str().unwrap();
            for invalid in [
                json!("wrong"),
                json!(literal.to_lowercase()),
                json!({(literal):null}),
                json!([literal]),
            ] {
                wrong[name] = invalid;
                assert!(
                    js_from_str::<CryptocomOrder>(&wrong.to_string()).is_err(),
                    "order_literal_shape_refusal:{name}"
                );
            }
        }
    }
    for invalid in [
        json!({}),
        json!("exec"),
        json!([1]),
        json!([null]),
        json!(["valid", false]),
    ] {
        let mut wrong = base.clone();
        wrong["exec_inst"] = invalid;
        assert!(
            js_from_str::<CryptocomOrder>(&wrong.to_string()).is_err(),
            "order_exec_array_refusal"
        );
    }
    let positional: Vec<Value> = fields
        .iter()
        .map(|field| base[field[0].as_str().unwrap()].clone())
        .collect();
    assert!(
        js_from_str::<CryptocomOrder>(&serde_json::to_string(&positional).unwrap()).is_err(),
        "order_populated_array_refusal"
    );
    assert_eq!(
        order["surrogateUnits"],
        json!([0xd800]),
        "order_ts_surrogate_retained"
    );
    typed_utf16::assert_projection(
        order["surrogateRaw"].as_str().unwrap(),
        &(js_from_str::<CryptocomOrder>(order["surrogateRaw"].as_str().unwrap()))
            .expect("typed surrogate record is retained"),
    );
    let mut nested = base.clone();
    nested["exec_inst"] = json!(["SURROGATE"]);
    let nested = nested.to_string().replace("SURROGATE", "\\ud800");
    typed_utf16::assert_projection(
        &nested,
        &(js_from_str::<CryptocomOrder>(&nested)).expect("typed surrogate record is retained"),
    );
    assert_eq!(order["extrasRetained"], true, "order_ts_extra_retained");
    let extra: CryptocomOrder = js_from_str(order["extrasRaw"].as_str().unwrap()).unwrap();
    assert_eq!(
        extra.exec_inst.as_ref().unwrap().len(),
        2,
        "order_extra_native_count"
    );
    assert_eq!(
        extra.additional_fields["__extra"].as_str(),
        Some("retained-in-TS"),
        "native_extra_retained:CryptocomOrder"
    );
    assert!(
        serde_json::to_value(extra)
            .unwrap()
            .get("__extra")
            .is_some(),
        "order_wire_extra_retained"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
