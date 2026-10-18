//! Independent native observations for each selected scalar decoder body.
use safe_cex_num::JsCoercionError;
use safe_cex_types::retained_json::RetainedJsonInput;
use safe_cex_venue_kraken::{fills, income};
use serde_json::{Value, json};

fn bits(result: Result<f64, JsCoercionError>, expected: f64) {
    assert_eq!(result.unwrap().to_bits(), expected.to_bits());
}
fn negative_zero() -> Value {
    RetainedJsonInput::Float(-0.0).into_value().unwrap()
}
fn optional_cases(decode: impl Fn(Option<&Value>) -> Result<Option<f64>, JsCoercionError>) {
    assert!(
        decode(None).unwrap().is_none(),
        "absent input remains absent"
    );
    for value in [
        Value::Null,
        json!(""),
        json!("Infinity"),
        json!("bad"),
        json!({}),
    ] {
        assert!(
            decode(Some(&value)).unwrap().is_none(),
            "not a finite optional number: {value}"
        );
    }
    for (value, expected) in [
        (negative_zero(), -0.0_f64),
        (json!(0), 0.0),
        (json!(false), 0.0),
        (json!(true), 1.0),
        (json!("  "), 0.0),
        (json!("-0"), -0.0),
        (json!("0x10"), 16.0),
        (json!("1.25e2"), 125.0),
        (json!([]), 0.0),
        (json!(["2.5"]), 2.5),
    ] {
        assert_eq!(
            decode(Some(&value))
                .unwrap()
                .expect("present finite number")
                .to_bits(),
            expected.to_bits(),
            "{value}"
        );
    }
    assert_eq!(
        decode(Some(&json!({"toString":null}))).unwrap_err().name,
        "TypeError"
    );
}

#[test]
fn fills_num_preserves_finite_bits_and_uses_positive_zero_fallback() {
    bits(fills::num(None), 0.0);
    for (value, expected) in [
        (negative_zero(), -0.0_f64),
        (json!(null), 0.0),
        (json!(false), 0.0),
        (json!(true), 1.0),
        (json!(""), 0.0),
        (json!("-0"), -0.0),
        (json!("0x10"), 16.0),
        (json!("-2.75"), -2.75),
        (json!("Infinity"), 0.0),
        (json!("bad"), 0.0),
        (json!([]), 0.0),
        (json!(["2.5"]), 2.5),
        (json!([1, 2]), 0.0),
        (json!({}), 0.0),
    ] {
        bits(fills::num(Some(&value)), expected);
    }
    assert_eq!(
        fills::num(Some(&json!({"toString":null})))
            .unwrap_err()
            .name,
        "TypeError"
    );
}

#[test]
fn fills_optional_num_observes_presence_before_native_finite_bits() {
    optional_cases(fills::optional_num);
}

#[test]
fn income_num_observes_presence_before_native_finite_bits() {
    optional_cases(income::num);
}

#[test]
fn fills_to_seconds_floors_before_epoch_and_uses_explicit_local_host() {
    bits(fills::to_seconds(None, |ms| ms), 0.0);
    for (value, expected) in [
        (json!("1970-01-01T00:00:01.999Z"), 1.0),
        (json!("1969-12-31T23:59:59.999Z"), -1.0),
        (json!("1970-01-01T00:00:00.999Z"), 0.0),
        (json!("1970-01-01T01:00:00+01:00"), 0.0),
        (json!(["1970-01-01T00:00:01.999Z"]), 1.0),
        (json!(null), 0.0),
        (json!("bad-date"), 0.0),
        (json!({}), 0.0),
    ] {
        bits(fills::to_seconds(Some(&value), |ms| ms), expected);
    }
    let mut calls = 0;
    bits(
        fills::to_seconds(Some(&json!("1970-01-01T03:00:01.999")), |ms| {
            calls += 1;
            assert_eq!(ms.to_bits(), 10_801_999.0_f64.to_bits());
            ms - 7_200_000.0
        }),
        3601.0,
    );
    assert_eq!(calls, 1);
    bits(
        fills::to_seconds(Some(&json!("1970-01-01T00:00:01Z")), |_| {
            panic!("UTC must not consult local host")
        }),
        1.0,
    );
    assert_eq!(
        fills::to_seconds(Some(&json!({"toString":null})), |ms| ms)
            .unwrap_err()
            .name,
        "TypeError"
    );
}

#[test]
fn income_to_ms_keeps_milliseconds_and_uses_explicit_local_host() {
    bits(income::to_ms(None, |ms| ms), 0.0);
    for (value, expected) in [
        (json!("1970-01-01T00:00:01.999Z"), 1999.0),
        (json!("1969-12-31T23:59:59.999Z"), -1.0),
        (json!("1970-01-01T00:00:00.999Z"), 999.0),
        (json!("1970-01-01T01:00:00+01:00"), 0.0),
        (json!(["1970-01-01T00:00:01.999Z"]), 1999.0),
        (json!(null), 0.0),
        (json!("bad-date"), 0.0),
        (json!({}), 0.0),
    ] {
        bits(income::to_ms(Some(&value), |ms| ms), expected);
    }
    let mut calls = 0;
    bits(
        income::to_ms(Some(&json!("1970-01-01T03:00:01.999")), |ms| {
            calls += 1;
            assert_eq!(ms.to_bits(), 10_801_999.0_f64.to_bits());
            ms - 7_200_000.0
        }),
        3_601_999.0,
    );
    assert_eq!(calls, 1);
    bits(
        income::to_ms(Some(&json!("1970-01-01T00:00:01Z")), |_| {
            panic!("UTC must not consult local host")
        }),
        1000.0,
    );
    assert_eq!(
        income::to_ms(Some(&json!({"toString":null})), |ms| ms)
            .unwrap_err()
            .name,
        "TypeError"
    );
}

#[test]
fn position_leg_decodes_fields_before_nonempty_case_insensitive_comparison() {
    for (value, expected) in [
        (json!({"asset":"PF_XRPUSD","contract":"pf_xrpusd"}), true),
        (json!({"asset":"usd","contract":"pf_xrpusd"}), false),
        (json!({"asset":"","contract":""}), false),
        (json!({"asset":null,"contract":null}), false),
        (json!({"asset":false,"contract":false}), true),
        (json!({"asset":0,"contract":0}), true),
        (json!({"asset":[],"contract":[]}), false),
        (json!({"asset":["PF_XRPUSD"],"contract":"pf_xrpusd"}), true),
        (json!({"asset":{},"contract":{}}), true),
        (json!({"asset":" usd ","contract":"usd"}), false),
        (json!({"asset":"USD"}), false),
        (json!(null), false),
        (json!([]), false),
    ] {
        assert_eq!(
            income::is_position_size_leg(&value).unwrap(),
            expected,
            "{value}"
        );
    }
    assert_eq!(
        income::is_position_size_leg(&json!({"asset":{"toString":null},"contract":"x"}))
            .unwrap_err()
            .name,
        "TypeError"
    );
}
