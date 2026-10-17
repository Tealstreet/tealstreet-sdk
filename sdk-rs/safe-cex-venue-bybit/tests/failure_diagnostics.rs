use safe_cex_venue_bybit::failure_diagnostics::*;
use serde_json::Value;
fn number(actual: f64, expected: &Value, label: &str) {
    let bits = if actual.is_nan() {
        "nan".into()
    } else {
        format!("{:016x}", actual.to_bits())
    };
    assert_eq!(
        bits,
        expected["number"].as_str().unwrap(),
        "DIAGNOSTIC native {label}"
    );
}
fn retained(actual: &Value, expected: &Value, label: &str) {
    match actual {
        Value::Number(_) => number(
            safe_cex_num::numeric_ingress::js_value_as_f64(actual).unwrap(),
            expected,
            label,
        ),
        Value::Array(a) => {
            let e = expected.as_array().unwrap();
            assert_eq!(
                a.len(),
                e.len(),
                "DIAGNOSTIC native source array length {label}"
            );
            for (a, e) in a.iter().zip(e) {
                retained(a, e, label);
            }
        }
        Value::Object(a) => {
            let e = expected.as_object().unwrap();
            assert_eq!(
                a.len(),
                e.len(),
                "DIAGNOSTIC native source object length {label}"
            );
            for (key, value) in a {
                retained(value, e.get(key).expect("source field"), label);
            }
        }
        _ => assert_eq!(actual, expected, "DIAGNOSTIC native {label}"),
    }
}
fn input(raw: &Value) -> Option<Value> {
    raw.as_str().map(|s| serde_json::from_str(s).unwrap())
}
fn optional_json(actual: Option<&Value>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "DIAGNOSTIC native absent {label}");
    } else {
        retained(actual.unwrap(), expected, label);
    }
}
fn optional_string(actual: Option<&str>, expected: &Value, label: &str) {
    if expected["undefined"] == true {
        assert!(actual.is_none(), "DIAGNOSTIC native absent {label}");
    } else {
        assert_eq!(actual, expected.as_str(), "DIAGNOSTIC native {label}");
    }
}
fn outcome<T>(actual: Result<T, &str>, expected: &Value) -> Option<T> {
    if let Some(e) = expected["message"].as_str() {
        assert_eq!(actual.err(), Some(e), "DIAGNOSTIC native coercion error");
        None
    } else {
        Some(actual.unwrap())
    }
}
#[test]
#[ignore = "requires installed live TypeScript"]
fn failure_diagnostics_native_source_parity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/failure-diagnostic-fixtures.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let f: Value = serde_json::from_slice(&output.stdout).unwrap();
    number(
        BYBIT_MARKETS_FAILURE_SENTRY_COOLDOWN_MS as f64,
        &f["cooldown"],
        "cooldown",
    );
    assert_eq!(
        f["failures"].as_array().unwrap().len(),
        36,
        "DIAGNOSTIC independent failure floor"
    );
    for c in f["failures"].as_array().unwrap() {
        let cause = input(&c["raw"]);
        if let Some(t) = outcome(
            get_bybit_markets_failure_telemetry(cause.as_ref()),
            &c["telemetry"],
        ) {
            let e = &c["telemetry"]["ok"];
            assert_eq!(
                t.reason,
                e["reason"].as_str().unwrap(),
                "DIAGNOSTIC native telemetry reason"
            );
            optional_json(
                t.http_status.as_ref(),
                &e["httpStatus"],
                "telemetry httpStatus",
            );
            optional_string(
                t.error_code.as_deref(),
                &e["errorCode"],
                "telemetry errorCode",
            );
            if e["retCode"]["undefined"] == true {
                assert!(t.ret_code.is_none(), "DIAGNOSTIC native absent retCode");
            } else {
                number(t.ret_code.unwrap(), &e["retCode"], "telemetry retCode");
            }
            optional_string(t.endpoint.as_deref(), &e["endpoint"], "telemetry endpoint");
            optional_string(t.method.as_deref(), &e["method"], "telemetry method");
        }
        if let Some(d) = outcome(
            describe_bybit_markets_failure(cause.as_ref()),
            &c["description"],
        ) {
            assert_eq!(
                d,
                c["description"]["ok"].as_str().unwrap(),
                "DIAGNOSTIC native description"
            );
        }
        if let Some(r) = outcome(
            is_retryable_bybit_trading_markets_failure(cause.as_ref()),
            &c["retry"],
        ) {
            assert_eq!(
                r,
                c["retry"]["ok"].as_bool().unwrap(),
                "DIAGNOSTIC native retry"
            );
        }
        if let Some(r) = outcome(
            BybitTradingMarketsRequestError::new(cause.as_ref(), Some(-0.)),
            &c["requestError"],
        ) {
            let e = &c["requestError"]["ok"];
            optional_json(r.cause.as_ref(), &e["cause"], "request cause");
            assert_eq!(
                r.name,
                e["name"].as_str().unwrap(),
                "DIAGNOSTIC native request name"
            );
            assert_eq!(
                r.message,
                e["message"].as_str().unwrap(),
                "DIAGNOSTIC native request message"
            );
            assert_eq!(
                r.to_string(),
                e["rendered"].as_str().unwrap(),
                "DIAGNOSTIC native request rendering"
            );
            assert_eq!(
                r.safe_cex_sentry_report_key,
                e["key"].as_str().unwrap(),
                "DIAGNOSTIC native request key"
            );
            number(
                r.safe_cex_sentry_report_cooldown_ms as f64,
                &e["cooldown"],
                "request cooldown",
            );
            retained(
                &Value::Object(r.safe_cex_sentry_tags),
                &e["tags"],
                "request tags",
            );
            retained(
                &Value::Object(r.safe_cex_sentry_extra),
                &e["extra"],
                "request extra",
            );
        }
    }
    assert_eq!(
        f["counters"].as_array().unwrap().len(),
        7,
        "DIAGNOSTIC independent counter floor"
    );
    for c in f["counters"].as_array().unwrap() {
        let token = c["token"].as_str().unwrap();
        let count = match token {
            "undefined" => None,
            "Infinity" => Some(f64::INFINITY),
            "NaN" => Some(f64::NAN),
            _ => Some(token.parse::<f64>().unwrap()),
        };
        let r = BybitTradingMarketsRequestError::new(Some(&serde_json::json!({})), count);
        if count.is_some_and(|v| !v.is_finite()) {
            assert!(r.is_err(), "DIAGNOSTIC native nonfinite counter refusal");
            continue;
        }
        let r = r.unwrap();
        let e = &c["result"]["ok"];
        retained(
            &Value::Object(r.safe_cex_sentry_tags),
            &e["tags"],
            "request default tags",
        );
        retained(
            &Value::Object(r.safe_cex_sentry_extra),
            &e["extra"],
            "request default extra",
        );
    }
    // Source succeeds with Infinity; retained JSON metadata deliberately refuses it.
    assert_eq!(
        f["retryBoundary"]["ok"]["retry"]["number"],
        "7ff0000000000000"
    );
    assert_eq!(
        BybitTradingMarketsRequestError::new(Some(&serde_json::json!({})), Some(f64::INFINITY))
            .unwrap_err(),
        "Bybit JSON diagnostic metadata requires a finite retry count",
        "DIAGNOSTIC native nonfinite retry refusal"
    );
    assert_eq!(
        f["fields"].as_array().unwrap().len(),
        20,
        "DIAGNOSTIC independent field floor"
    );
    for c in f["fields"].as_array().unwrap() {
        let payload = input(&c["raw"]);
        let keys = c["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|k| k.as_str().unwrap())
            .collect::<Vec<_>>();
        let r = get_bybit_error_field(payload.as_ref(), &keys);
        if c["refusal"] == true {
            assert!(r.is_err(), "DIAGNOSTIC native inherited field refusal");
        } else if let Some(r) = outcome(r, &c["result"]) {
            optional_json(r.as_ref(), &c["result"]["ok"], "error field");
        }
    }
    assert_eq!(
        f["leverage"].as_array().unwrap().len(),
        12,
        "DIAGNOSTIC independent leverage floor"
    );
    for c in f["leverage"].as_array().unwrap() {
        let value = input(&c["raw"]);
        let data = value.as_ref().and_then(Value::as_object);
        if let Some(r) = outcome(is_bybit_leverage_not_modified(data), &c["result"]) {
            assert_eq!(
                r,
                c["result"]["ok"].as_bool().unwrap(),
                "DIAGNOSTIC native leverage"
            );
        }
    }
    assert_eq!(
        f["responses"].as_array().unwrap().len(),
        14,
        "DIAGNOSTIC independent response floor"
    );
    let mut accepted = 0;
    let mut compiler_refused = 0;
    let mut native_refused = 0;
    for c in f["responses"].as_array().unwrap() {
        let r = serde_json::from_str::<BybitCreateOrderResponse>(c["raw"].as_str().unwrap());
        if c["accepted"] == false {
            assert!(
                r.is_err(),
                "DIAGNOSTIC native compiler-invalid response refusal"
            );
            compiler_refused += 1;
            continue;
        }
        if c["nativeRefusal"] == true {
            assert!(
                r.is_err(),
                "DIAGNOSTIC native response record-array refusal"
            );
            native_refused += 1;
            continue;
        }
        let r = r.unwrap();
        let e = &c["value"];
        retained(&r.data, &e["data"], "response data");
        retained(&Value::Object(r.payload), &e["payload"], "response payload");
        assert_eq!(
            r.terminal,
            e["terminal"].as_bool(),
            "DIAGNOSTIC native response terminal"
        );
        assert_eq!(
            r.recorded,
            e["recorded"].as_bool(),
            "DIAGNOSTIC native response recorded"
        );
        assert_eq!(
            r.reconciled,
            e["reconciled"].as_bool(),
            "DIAGNOSTIC native response reconciled"
        );
        assert_eq!(
            r.authoritative_seen,
            e["authoritativeSeen"].as_bool(),
            "DIAGNOSTIC native response authoritativeSeen"
        );
        match r.recovered_order {
            Some(map) => retained(
                &Value::Object(map),
                &e["recoveredOrder"],
                "response recoveredOrder",
            ),
            None => assert!(
                e.get("recoveredOrder").is_none(),
                "DIAGNOSTIC native absent recoveredOrder"
            ),
        };
        accepted += 1;
    }
    assert_eq!(
        (accepted, compiler_refused, native_refused),
        (4, 8, 2),
        "DIAGNOSTIC independent response outcomes"
    );
}
