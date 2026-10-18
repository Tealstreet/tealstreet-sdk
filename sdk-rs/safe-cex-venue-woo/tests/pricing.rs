use safe_cex_num::JsCoercionError;
use safe_cex_types::{Market, OrderSide, Ticker};
use safe_cex_venue_woo::pricing::*;
use serde_json::{Value, json};
fn number(v: &Value) -> f64 {
    match v.get("$number").and_then(Value::as_str) {
        Some("NaN") => f64::NAN,
        Some("Infinity") => f64::INFINITY,
        Some("-Infinity") => f64::NEG_INFINITY,
        Some("-0") => -0.0,
        _ => v.as_f64().unwrap(),
    }
}
fn absent(v: &Value) -> bool {
    v.get("$undefined") == Some(&Value::Bool(true))
}
fn encoded(v: f64) -> Value {
    if v.is_nan() {
        json!({"$number":"NaN"})
    } else if v == f64::INFINITY {
        json!({"$number":"Infinity"})
    } else if v == f64::NEG_INFINITY {
        json!({"$number":"-Infinity"})
    } else if v == 0.0 && v.is_sign_negative() {
        json!({"$number":"-0"})
    } else {
        json!(v)
    }
}
fn optional(v: Option<f64>) -> Value {
    v.map_or(json!({"$undefined":true}), encoded)
}
fn ticker(v: &Value) -> Option<Ticker> {
    if absent(v) {
        return None;
    }
    let read = |k| v.get(k).map(number).unwrap_or(f64::NAN);
    Some(Ticker {
        id: "x".into(),
        symbol: "X".into(),
        bid: read("bid"),
        ask: read("ask"),
        last: read("last"),
        mark: read("mark"),
        index: Some(read("index")),
        mid_price: v.get("midPrice").map(number),
        timestamp: v.get("timestamp").map(number),
        percentage: 0.0,
        open_interest: 0.0,
        funding_rate: 0.0,
        funding_time: 0.0,
        volume: 0.0,
        quote_volume: 0.0,
        info: None,
    })
}
fn market(v: &Value) -> Market {
    let mut complete = json!({"id":"x","symbol":"X","base":"X","quote":"USDT","active":true,"precision":{"amount":1,"price":1},"limits":{"amount":{"min":1,"max":100},"leverage":{"min":1,"max":100}}});
    complete
        .as_object_mut()
        .unwrap()
        .extend(v.as_object().unwrap().clone());
    if complete.pointer("/info/price_range").is_some_and(absent) {
        complete["info"]
            .as_object_mut()
            .unwrap()
            .remove("price_range");
    }
    serde_json::from_value(complete).unwrap()
}
fn same(a: &Value, b: &Value) {
    match (a, b) {
        (Value::Number(a), Value::Number(b)) => assert_eq!(a.as_f64(), b.as_f64()),
        (Value::Object(a), Value::Object(b)) => {
            assert_eq!(a.len(), b.len());
            for (k, v) in a {
                same(v, &b[k]);
            }
        }
        _ => assert_eq!(a, b),
    }
}
#[test]
fn pricing_and_errors_match_live_source_including_nonfinite_numbers() {
    let cases: Value = serde_json::from_str(include_str!("fixtures/pricing.json")).unwrap();
    assert_eq!(
        cases.as_array().unwrap().len(),
        2071,
        "WOO-CARDINALITY pricing source total"
    );
    for case in cases.as_array().unwrap() {
        let a = case["args"].as_array().unwrap();
        let side = |i: usize| serde_json::from_value::<OrderSide>(a[i].clone()).unwrap();
        let result: Result<Value, JsCoercionError> = match case["name"].as_str().unwrap() {
            "isPositiveFinitePrice" => Ok(json!(is_positive_finite_price(
                (!absent(&a[0])).then(|| number(&a[0]))
            ))),
            "applyMarketIocSlippage" => Ok(encoded(apply_market_ioc_slippage(
                number(&a[0]),
                side(1),
                number(&a[2]),
            ))),
            "chooseMarketIocOrderPrice" => Ok(optional(choose_market_ioc_order_price(
                ticker(&a[0]).as_ref(),
                side(1),
                number(&a[2]),
                number(&a[3]),
                (!absent(&a[4])).then(|| number(&a[4])),
            ))),
            "getWooMarketPriceRangePct" => {
                get_woo_market_price_range_pct(Some(&market(&a[0]))).map(encoded)
            }
            "isWooRateLimitError" => is_woo_rate_limit_error(Some(&a[0])).map(|v| json!(v)),
            "isWooPositionModeAlreadySetError" => {
                is_woo_position_mode_already_set_error(Some(&a[0])).map(|v| json!(v))
            }
            "isPostOnlyCrossingPrice" => Ok(json!(is_post_only_crossing_price(
                side(0),
                number(&a[1]),
                number(&a[2])
            ))),
            "isPostOnlyCrossingFairPrice" => Ok(is_post_only_crossing_fair_price(
                side(0),
                number(&a[1]),
                ticker(&a[2]).as_ref(),
            )
            .map_or(
                json!({"$undefined":true}),
                |(fair, cross)| json!({"fairPrice":fair,"wouldCross":cross}),
            )),
            name => panic!("unclaimed source {name}"),
        };
        if let Some(error) = case.get("error") {
            let actual = result.unwrap_err();
            assert_eq!(actual.message, error.as_str().unwrap(), "{case}");
            assert_eq!(actual.name, case["errorName"].as_str().unwrap());
        } else {
            let actual = result.unwrap_or_else(|e| panic!("{case}: {e:?}"));
            same(&actual, &case["ok"]);
        }
    }
}

#[test]
fn error_guards_native_before_source() {
    let poison = json!({"toString":null,"valueOf":null});
    for response in [
        json!({"status":429}),
        json!({"data":{"code":-1003}}),
        json!({"data":{"error_code":90003}}),
    ] {
        assert!(
            is_woo_rate_limit_error(Some(&json!({"response":response,"message":poison}))).unwrap(),
            "WOO-GUARD native strict numeric short circuit"
        );
    }
    for response in [
        json!({"status":"429"}),
        json!({"data":{"code":"-1003"}}),
        json!({"data":{"error_code":"90003"}}),
    ] {
        assert!(!is_woo_rate_limit_error(Some(&json!({"response":response}))).unwrap());
    }
    assert!(
        is_woo_rate_limit_error(Some(
            &json!({"response":{"data":{"message":0}},"message":"RATE LIMIT"})
        ))
        .unwrap()
    );
    assert!(
        !is_woo_rate_limit_error(Some(
            &json!({"response":{"data":{"message":"other"}},"message":"RATE LIMIT"})
        ))
        .unwrap()
    );
    assert!(is_woo_position_mode_already_set_error(Some(&json!({"response":{"data":{"message":false}},"message":"POSITION MODE REMAINS UNCHANGED"}))).unwrap());
    assert!(!is_woo_position_mode_already_set_error(Some(&json!({"response":{"data":{"message":"other"}},"message":"POSITION MODE REMAINS UNCHANGED"}))).unwrap());
    let error = is_woo_rate_limit_error(Some(&json!({"message":poison}))).unwrap_err();
    assert_eq!(error.name, "TypeError");
    assert!(!is_woo_rate_limit_error(Some(&json!({}))).unwrap());

    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/pricing-oracle.cjs"
        ))
        .arg("--guard-audit")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "WOO-GUARD source: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let value: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        value["groups"], 2,
        "WOO-GUARD independently fixed source groups"
    );
}
