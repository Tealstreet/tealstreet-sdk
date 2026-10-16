//! Native DTO retention assertions precede source comparisons and all wire assertions.
use safe_cex_num::numeric_ingress::{
    js_from_str, js_from_value, js_from_value_ref, js_value_as_f64,
};
use safe_cex_venue_cryptocom::wire::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn fixtures() -> Value {
    input_value(include_str!("dto_fixtures.json"))
}
fn input_value(raw: &str) -> Value {
    serde_json::from_str::<safe_cex_types::retained_json::RetainedJsonInput>(raw)
        .unwrap()
        .into_value()
        .unwrap()
}
fn admitted<T>(result: Result<T, serde_json::Error>, route: &str) -> T {
    result.unwrap_or_else(|error| panic!("DTO-ADMISSION:{route}:{error}"))
}
fn source() -> &'static Value {
    static VALUE: OnceLock<Value> = OnceLock::new();
    VALUE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/dto_oracle.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "DTO-SOURCE-EXIT:{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
const EXTRA_KEYS: [&str; 16] = [
    "1",
    "2",
    "4294967294",
    "z",
    "__middle",
    "01",
    "4294967295",
    "__duplicate",
    "__overflow",
    "__negativeZero",
    "__underflow",
    "__numberObject",
    "__nested",
    "__proto__",
    "additional_fields",
    "input_keys",
];
fn assert_native(extra: &Value, keys: Vec<String>, fixture: &Value, label: &str) {
    let extra = extra.as_object().expect("DTO-NATIVE-EXTRA-OBJECT");
    assert_eq!(extra.len(), 16, "DTO-NATIVE-COUNT:{label}");
    assert_eq!(
        extra.keys().map(String::as_str).collect::<Vec<_>>(),
        EXTRA_KEYS,
        "DTO-NATIVE-KEYS:{label}"
    );
    assert_eq!(
        keys,
        fixture["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect::<Vec<_>>(),
        "DTO-NATIVE-ORDER:{label}"
    );
    for (key, expected) in [
        ("z", "first-extra"),
        ("2", "two"),
        ("1", "one"),
        ("__middle", "middle-extra"),
        ("01", "leading-zero"),
        ("4294967295", "not-index"),
        ("4294967294", "last-index"),
        ("__duplicate", "last"),
        ("additional_fields", "source-data"),
    ] {
        assert_eq!(
            extra[key].as_str(),
            Some(expected),
            "DTO-NATIVE-VALUE:{label}:{key}"
        );
    }
    assert_eq!(
        extra["input_keys"].as_array().unwrap()[0].as_str(),
        Some("source-data"),
        "DTO-NATIVE-METADATA-NAME:{label}"
    );
    assert_eq!(
        js_value_as_f64(&extra["__overflow"]).unwrap().to_bits(),
        f64::INFINITY.to_bits(),
        "DTO-NATIVE-OVERFLOW:{label}"
    );
    assert_eq!(
        js_value_as_f64(&extra["__negativeZero"]).unwrap().to_bits(),
        (-0.0f64).to_bits(),
        "DTO-NATIVE-NEGATIVE-ZERO:{label}"
    );
    assert_eq!(
        js_value_as_f64(&extra["__underflow"]).unwrap().to_bits(),
        (-0.0f64).to_bits(),
        "DTO-NATIVE-UNDERFLOW:{label}"
    );
    assert_eq!(
        extra["__numberObject"].as_object().unwrap().len(),
        1,
        "DTO-NATIVE-NUMBER-OBJECT:{label}"
    );
    assert_eq!(
        extra["__numberObject"]["$serde_json::private::Number"].as_str(),
        Some("ordinary-object"),
        "DTO-NATIVE-NUMBER-OBJECT:{label}"
    );
    let nested = extra["__nested"].as_object().unwrap();
    assert_eq!(nested.len(), 6, "DTO-NATIVE-NESTED-COUNT:{label}");
    assert_eq!(
        nested.keys().map(String::as_str).collect::<Vec<_>>(),
        ["1", "2", "z", "01", "infinite", "zero"],
        "DTO-NATIVE-NESTED-ORDER:{label}"
    );
    assert_eq!(
        js_value_as_f64(&nested["1"]).unwrap().to_bits(),
        1.0f64.to_bits()
    );
    assert_eq!(
        js_value_as_f64(&nested["2"]).unwrap().to_bits(),
        2.0f64.to_bits()
    );
    assert_eq!(nested["z"].as_str(), Some("tail"));
    assert_eq!(nested["01"].as_str(), Some("ordinary"));
    assert_eq!(
        js_value_as_f64(&nested["infinite"]).unwrap().to_bits(),
        f64::NEG_INFINITY.to_bits(),
        "DTO-NATIVE-NESTED-INFINITY:{label}"
    );
    assert_eq!(
        js_value_as_f64(&nested["zero"]).unwrap().to_bits(),
        (-0.0f64).to_bits(),
        "DTO-NATIVE-NESTED-ZERO:{label}"
    );
    assert_eq!(
        extra["__proto__"]["safe"].as_str(),
        Some("own-data"),
        "DTO-NATIVE-PROTO-DATA:{label}"
    );
}
fn assert_wire_record(encoded: &Value, fixture: &Value, label: &str) {
    assert_eq!(
        encoded
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        fixture["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect::<Vec<_>>(),
        "DTO-WIRE-ORDER:{label}"
    );
    assert_eq!(
        encoded.get("__overflow"),
        Some(&Value::Null),
        "DTO-WIRE-INFINITY-PROJECTION:{label}"
    );
    assert_eq!(
        encoded["__duplicate"].as_str(),
        Some("last"),
        "DTO-WIRE-DUPLICATE:{label}"
    );
    assert_eq!(
        encoded["__middle"].as_str(),
        Some("middle-extra"),
        "DTO-WIRE-EXTRA:{label}"
    );
    assert_eq!(
        encoded["__nested"]
            .as_object()
            .unwrap()
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        ["1", "2", "z", "01", "infinite", "zero"],
        "DTO-WIRE-EXTRA-ORDER:{label}"
    );
    assert_eq!(
        encoded["__nested"].get("infinite"),
        Some(&Value::Null),
        "DTO-WIRE-NESTED-INFINITY:{label}"
    );
    assert_eq!(
        encoded["__numberObject"]["$serde_json::private::Number"].as_str(),
        Some("ordinary-object")
    );
    assert_eq!(encoded["__proto__"]["safe"].as_str(), Some("own-data"));
    assert_eq!(encoded["additional_fields"].as_str(), Some("source-data"));
    assert_eq!(encoded["input_keys"][0].as_str(), Some("source-data"));
}
fn assert_wire<T: serde::Serialize>(
    model: &T,
    fixture: &Value,
    first: &str,
    edited: &Value,
    label: &str,
) {
    // Only reached after every decoded native assertion for every admitted route.
    let encoded = serde_json::to_value(model).unwrap();
    assert_wire_record(&encoded, fixture, label);
    assert_eq!(encoded[first], *edited, "DTO-WIRE-CURRENT-KNOWN:{label}");
}

#[test]
fn dto_instrument_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[0];
    assert_eq!(
        fixture["name"], "CryptocomInstrument",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomInstrument> = vec![
        admitted(js_from_str(raw), "CryptocomInstrument:raw"),
        admitted(js_from_value(input.clone()), "CryptocomInstrument:owned"),
        admitted(js_from_value_ref(&input), "CryptocomInstrument:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomInstrument:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomInstrument",
        );
        assert_eq!(
            model.symbol,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomInstrument"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.symbol = "native-edit".into();
        assert_eq!(
            model.symbol, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomInstrument"
        );
    }
    assert_eq!(
        source()["names"][0],
        "CryptocomInstrument",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][0]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomInstrument"
    );
    assert_eq!(
        source()["rows"][0]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomInstrument"
    );
    assert_eq!(
        source()["rows"][0]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomInstrument"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "symbol",
            &json!("native-edit"),
            "CryptocomInstrument",
        );
    }
}

#[test]
fn dto_book_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[1];
    assert_eq!(
        fixture["name"], "CryptocomBook",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomBook> = vec![
        admitted(js_from_str(raw), "CryptocomBook:raw"),
        admitted(js_from_value(input.clone()), "CryptocomBook:owned"),
        admitted(js_from_value_ref(&input), "CryptocomBook:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomBook:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomBook",
        );
        assert_eq!(
            model.instrument_name,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomBook"
        );
        assert_eq!(model.data.len(), 2, "DTO-NATIVE-CHILD-COUNT:CryptocomBook");
        for (index, child) in model.data.iter().enumerate() {
            assert_native(
                &child.additional_fields,
                child.own_keys(),
                &fixture["nested"][index],
                "CryptocomBook:child",
            );
        }
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.instrument_name = "native-edit".into();
        assert_eq!(
            model.instrument_name, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomBook"
        );
    }
    assert_eq!(source()["names"][1], "CryptocomBook", "DTO-SOURCE-CASE");
    assert_eq!(
        source()["rows"][1]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomBook"
    );
    assert_eq!(
        source()["rows"][1]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomBook"
    );
    assert_eq!(
        source()["rows"][1]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomBook"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "instrument_name",
            &json!("native-edit"),
            "CryptocomBook",
        );
        let encoded = serde_json::to_value(model).unwrap();
        assert_eq!(
            encoded["data"].as_array().unwrap().len(),
            2,
            "DTO-WIRE-CHILD-COUNT:CryptocomBook"
        );
        for index in 0..2 {
            assert_wire_record(
                &encoded["data"][index],
                &fixture["nested"][index],
                "CryptocomBook:child",
            );
        }
    }
}

#[test]
fn dto_ticker_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[2];
    assert_eq!(
        fixture["name"], "CryptocomTicker",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomTicker> = vec![
        admitted(js_from_str(raw), "CryptocomTicker:raw"),
        admitted(js_from_value(input.clone()), "CryptocomTicker:owned"),
        admitted(js_from_value_ref(&input), "CryptocomTicker:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomTicker:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomTicker",
        );
        assert_eq!(
            model.i,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomTicker"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.i = "native-edit".into();
        assert_eq!(model.i, "native-edit", "DTO-NATIVE-EDIT:CryptocomTicker");
    }
    assert_eq!(source()["names"][2], "CryptocomTicker", "DTO-SOURCE-CASE");
    assert_eq!(
        source()["rows"][2]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomTicker"
    );
    assert_eq!(
        source()["rows"][2]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomTicker"
    );
    assert_eq!(
        source()["rows"][2]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomTicker"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "i",
            &json!("native-edit"),
            "CryptocomTicker",
        );
    }
}

#[test]
fn dto_trade_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[3];
    assert_eq!(
        fixture["name"], "CryptocomTrade",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomTrade> = vec![
        admitted(js_from_str(raw), "CryptocomTrade:raw"),
        admitted(js_from_value(input.clone()), "CryptocomTrade:owned"),
        admitted(js_from_value_ref(&input), "CryptocomTrade:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomTrade:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomTrade",
        );
        assert_eq!(
            model.d.to_bits(),
            fixture["firstValue"].as_f64().unwrap().to_bits(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomTrade"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.d = 123.5;
        assert_eq!(
            model.d.to_bits(),
            123.5f64.to_bits(),
            "DTO-NATIVE-EDIT:CryptocomTrade"
        );
    }
    assert_eq!(source()["names"][3], "CryptocomTrade", "DTO-SOURCE-CASE");
    assert_eq!(
        source()["rows"][3]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomTrade"
    );
    assert_eq!(
        source()["rows"][3]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomTrade"
    );
    assert_eq!(
        source()["rows"][3]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomTrade"
    );
    for model in &models {
        assert_wire(model, fixture, "d", &json!(123.5), "CryptocomTrade");
    }
}

#[test]
fn dto_candlestick_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[4];
    assert_eq!(
        fixture["name"], "CryptocomCandlestick",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomCandlestick> = vec![
        admitted(js_from_str(raw), "CryptocomCandlestick:raw"),
        admitted(js_from_value(input.clone()), "CryptocomCandlestick:owned"),
        admitted(js_from_value_ref(&input), "CryptocomCandlestick:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomCandlestick:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomCandlestick",
        );
        assert_eq!(
            model.t.to_bits(),
            fixture["firstValue"].as_f64().unwrap().to_bits(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomCandlestick"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.t = 123.5;
        assert_eq!(
            model.t.to_bits(),
            123.5f64.to_bits(),
            "DTO-NATIVE-EDIT:CryptocomCandlestick"
        );
    }
    assert_eq!(
        source()["names"][4],
        "CryptocomCandlestick",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][4]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomCandlestick"
    );
    assert_eq!(
        source()["rows"][4]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomCandlestick"
    );
    assert_eq!(
        source()["rows"][4]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomCandlestick"
    );
    for model in &models {
        assert_wire(model, fixture, "t", &json!(123.5), "CryptocomCandlestick");
    }
}

#[test]
fn dto_position_balance_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[5];
    assert_eq!(
        fixture["name"], "CryptocomPositionBalance",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomPositionBalance> = vec![
        admitted(js_from_str(raw), "CryptocomPositionBalance:raw"),
        admitted(
            js_from_value(input.clone()),
            "CryptocomPositionBalance:owned",
        ),
        admitted(
            js_from_value_ref(&input),
            "CryptocomPositionBalance:borrowed",
        ),
        admitted(
            serde_json::from_str(raw),
            "CryptocomPositionBalance:serde-raw",
        ),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomPositionBalance",
        );
        assert_eq!(
            model.instrument_name,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomPositionBalance"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.instrument_name = "native-edit".into();
        assert_eq!(
            model.instrument_name, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomPositionBalance"
        );
    }
    assert_eq!(
        source()["names"][5],
        "CryptocomPositionBalance",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][5]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomPositionBalance"
    );
    assert_eq!(
        source()["rows"][5]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomPositionBalance"
    );
    assert_eq!(
        source()["rows"][5]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomPositionBalance"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "instrument_name",
            &json!("native-edit"),
            "CryptocomPositionBalance",
        );
    }
}

#[test]
fn dto_balance_summary_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[6];
    assert_eq!(
        fixture["name"], "CryptocomBalanceSummary",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomBalanceSummary> = vec![
        admitted(js_from_str(raw), "CryptocomBalanceSummary:raw"),
        admitted(
            js_from_value(input.clone()),
            "CryptocomBalanceSummary:owned",
        ),
        admitted(
            js_from_value_ref(&input),
            "CryptocomBalanceSummary:borrowed",
        ),
        admitted(
            serde_json::from_str(raw),
            "CryptocomBalanceSummary:serde-raw",
        ),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomBalanceSummary",
        );
        assert_eq!(
            model.total_available_balance,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomBalanceSummary"
        );
        assert_eq!(
            model.position_balances.len(),
            2,
            "DTO-NATIVE-CHILD-COUNT:CryptocomBalanceSummary"
        );
        for (index, child) in model.position_balances.iter().enumerate() {
            assert_native(
                &child.additional_fields,
                child.own_keys(),
                &fixture["nested"][index],
                "CryptocomBalanceSummary:child",
            );
        }
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.total_available_balance = "native-edit".into();
        assert_eq!(
            model.total_available_balance, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomBalanceSummary"
        );
    }
    assert_eq!(
        source()["names"][6],
        "CryptocomBalanceSummary",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][6]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomBalanceSummary"
    );
    assert_eq!(
        source()["rows"][6]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomBalanceSummary"
    );
    assert_eq!(
        source()["rows"][6]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomBalanceSummary"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "total_available_balance",
            &json!("native-edit"),
            "CryptocomBalanceSummary",
        );
        let encoded = serde_json::to_value(model).unwrap();
        assert_eq!(
            encoded["position_balances"].as_array().unwrap().len(),
            2,
            "DTO-WIRE-CHILD-COUNT:CryptocomBalanceSummary"
        );
        for index in 0..2 {
            assert_wire_record(
                &encoded["position_balances"][index],
                &fixture["nested"][index],
                "CryptocomBalanceSummary:child",
            );
        }
    }
}

#[test]
fn dto_order_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[7];
    assert_eq!(
        fixture["name"], "CryptocomOrder",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomOrder> = vec![
        admitted(js_from_str(raw), "CryptocomOrder:raw"),
        admitted(js_from_value(input.clone()), "CryptocomOrder:owned"),
        admitted(js_from_value_ref(&input), "CryptocomOrder:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomOrder:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomOrder",
        );
        assert_eq!(
            model.account_id.as_deref(),
            fixture["firstValue"].as_str(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomOrder"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.account_id = Some("native-edit".into());
        assert_eq!(
            model.account_id.as_deref(),
            Some("native-edit"),
            "DTO-NATIVE-EDIT:CryptocomOrder"
        );
    }
    assert_eq!(source()["names"][7], "CryptocomOrder", "DTO-SOURCE-CASE");
    assert_eq!(
        source()["rows"][7]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomOrder"
    );
    assert_eq!(
        source()["rows"][7]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomOrder"
    );
    assert_eq!(
        source()["rows"][7]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomOrder"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "account_id",
            &json!("native-edit"),
            "CryptocomOrder",
        );
    }
}

#[test]
fn dto_accounts_response_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[8];
    assert_eq!(
        fixture["name"], "CryptocomAccountsResponse",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomAccountsResponse> = vec![
        admitted(js_from_str(raw), "CryptocomAccountsResponse:raw"),
        admitted(
            js_from_value(input.clone()),
            "CryptocomAccountsResponse:owned",
        ),
        admitted(
            js_from_value_ref(&input),
            "CryptocomAccountsResponse:borrowed",
        ),
        admitted(
            serde_json::from_str(raw),
            "CryptocomAccountsResponse:serde-raw",
        ),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomAccountsResponse",
        );
        assert_eq!(
            model.this_account.uuid,
            fixture["nested"][0]["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomAccountsResponse"
        );
        assert_native(
            &model.this_account.additional_fields,
            model.this_account.own_keys(),
            &fixture["nested"][0],
            "CryptocomAccountsResponse:child",
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.this_account.uuid = "native-edit".into();
        assert_eq!(
            model.this_account.uuid, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomAccountsResponse"
        );
    }
    assert_eq!(
        source()["names"][8],
        "CryptocomAccountsResponse",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][8]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomAccountsResponse"
    );
    assert_eq!(
        source()["rows"][8]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomAccountsResponse"
    );
    assert_eq!(
        source()["rows"][8]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomAccountsResponse"
    );
    for model in &models {
        let encoded = serde_json::to_value(model).unwrap();
        assert_wire_record(&encoded, fixture, "CryptocomAccountsResponse");
        assert_wire_record(
            &encoded["this_account"],
            &fixture["nested"][0],
            "CryptocomAccountsResponse:child",
        );
        assert_eq!(
            encoded["this_account"]["uuid"].as_str(),
            Some("native-edit"),
            "DTO-WIRE-CURRENT-NESTED"
        );
    }
}

#[test]
fn dto_trade_fill_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[9];
    assert_eq!(
        fixture["name"], "CryptocomTradeFill",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomTradeFill> = vec![
        admitted(js_from_str(raw), "CryptocomTradeFill:raw"),
        admitted(js_from_value(input.clone()), "CryptocomTradeFill:owned"),
        admitted(js_from_value_ref(&input), "CryptocomTradeFill:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomTradeFill:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomTradeFill",
        );
        assert_eq!(
            model.account_id,
            fixture["firstValue"].as_str().unwrap(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomTradeFill"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.account_id = "native-edit".into();
        assert_eq!(
            model.account_id, "native-edit",
            "DTO-NATIVE-EDIT:CryptocomTradeFill"
        );
    }
    assert_eq!(
        source()["names"][9],
        "CryptocomTradeFill",
        "DTO-SOURCE-CASE"
    );
    assert_eq!(
        source()["rows"][9]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomTradeFill"
    );
    assert_eq!(
        source()["rows"][9]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomTradeFill"
    );
    assert_eq!(
        source()["rows"][9]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomTradeFill"
    );
    for model in &models {
        assert_wire(
            model,
            fixture,
            "account_id",
            &json!("native-edit"),
            "CryptocomTradeFill",
        );
    }
}

#[test]
fn dto_error_retains_unknown_fields() {
    let inventory = fixtures();
    let fixture = &inventory[10];
    assert_eq!(
        fixture["name"], "CryptocomError",
        "DTO-INDEPENDENT-CASE-IDENTITY"
    );
    let raw = fixture["raw"].as_str().unwrap();
    let input = input_value(raw);
    let mut models: Vec<CryptocomError> = vec![
        admitted(js_from_str(raw), "CryptocomError:raw"),
        admitted(js_from_value(input.clone()), "CryptocomError:owned"),
        admitted(js_from_value_ref(&input), "CryptocomError:borrowed"),
        admitted(serde_json::from_str(raw), "CryptocomError:serde-raw"),
    ];
    for model in &models {
        assert_native(
            &model.additional_fields,
            model.own_keys(),
            fixture,
            "CryptocomError",
        );
        assert_eq!(
            model.code.to_bits(),
            fixture["firstValue"].as_f64().unwrap().to_bits(),
            "DTO-NATIVE-KNOWN-LAST:CryptocomError"
        );
    }
    // In-place typed updates must drive projection, not an old captured known value.
    for model in &mut models {
        model.code = 123.5;
        assert_eq!(
            model.code.to_bits(),
            123.5f64.to_bits(),
            "DTO-NATIVE-EDIT:CryptocomError"
        );
    }
    assert_eq!(source()["names"][10], "CryptocomError", "DTO-SOURCE-CASE");
    assert_eq!(
        source()["rows"][10]["keys"],
        fixture["keys"],
        "DTO-SOURCE-ORDER:CryptocomError"
    );
    assert_eq!(
        source()["rows"][10]["extraCount"],
        16,
        "DTO-SOURCE-EXTRA-COUNT:CryptocomError"
    );
    assert_eq!(
        source()["rows"][10]["bits"],
        json!(["7ff0000000000000", "8000000000000000", "8000000000000000"]),
        "DTO-SOURCE-NUMBERS:CryptocomError"
    );
    for model in &models {
        assert_wire(model, fixture, "code", &json!(123.5), "CryptocomError");
    }
}
