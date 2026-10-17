use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_types::object_map::JsObjectMap;
use safe_cex_venue_woo::{
    v3_account_writes::*,
    v3_contracts::{WooV3Endpoint, WooV3HttpMethod},
    v3_request::WooV3RequestInput,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-leverage-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "ACCOUNT-LEVERAGE source oracle {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
#[test]
fn error_guards_and_account_validators_native_before_source() {
    use safe_cex_venue_woo::pricing::{
        is_woo_position_mode_already_set_error, is_woo_rate_limit_error,
    };
    use serde_json::json;
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

    let record: JsNumberInput = js_from_str(r#"{"n":-0}"#).unwrap();
    let mut owned = require_record(Some(&record), "guard").unwrap();
    assert_eq!(owned.len(), 1);
    assert_eq!(
        owned
            .get("n")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
    owned.insert("n".into(), js_from_str("37").unwrap());
    let original = record
        .clone()
        .decode::<JsObjectMap<JsNumberInput>>()
        .unwrap();
    assert_eq!(
        original
            .get("n")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits(),
        "WOO-GUARD native record snapshot"
    );
    let array: JsNumberInput = js_from_str("[]").unwrap();
    assert_eq!(
        require_record(Some(&array), "guard").unwrap_err(),
        "guard must be an object"
    );
    let yes = js_from_str::<JsNumberInput>("true").unwrap();
    let no = js_from_str::<JsNumberInput>("false").unwrap();
    let zero = js_from_str::<JsNumberInput>("-0").unwrap();
    assert!(require_boolean(Some(&yes), "guard").unwrap());
    assert!(!require_boolean(Some(&no), "guard").unwrap());
    assert_eq!(
        require_boolean(Some(&zero), "guard").unwrap_err(),
        "guard must be a boolean"
    );
    assert_eq!(
        require_integer(Some(&zero), "guard", f64::NAN)
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits(),
        "WOO-GUARD native NaN minimum keeps negative zero"
    );
    let safe = js_from_str::<JsNumberInput>("9007199254740991").unwrap();
    let unsafe_int = js_from_str::<JsNumberInput>("9007199254740992").unwrap();
    assert_eq!(
        require_integer(Some(&safe), "guard", 0.0)
            .unwrap()
            .to_bits(),
        9_007_199_254_740_991.0f64.to_bits()
    );
    assert_eq!(
        require_integer(Some(&unsafe_int), "guard", 0.0).unwrap_err(),
        "guard must be a safe integer of at least 0"
    );
    assert_eq!(
        require_integer(Some(&safe), "guard", f64::INFINITY).unwrap_err(),
        "guard must be a safe integer of at least Infinity"
    );
    let whitespace = js_from_str::<JsNumberInput>(r#"" \t""#).unwrap();
    assert_eq!(require_string(Some(&whitespace), "guard").unwrap(), " \t");
    let empty = js_from_str::<JsNumberInput>(r#""""#).unwrap();
    assert_eq!(
        require_string(Some(&empty), "guard").unwrap_err(),
        "guard must be a non-empty string"
    );
    let b = js_from_str::<JsNumberInput>(r#""B""#).unwrap();
    assert_eq!(require_enum(Some(&b), &["B", "A"], "guard").unwrap(), "B");
    assert_eq!(
        require_enum(Some(&b), &["A"], "guard").unwrap_err(),
        "guard must be one of A",
        "WOO-GUARD native captured allowed list"
    );
    assert_eq!(
        require_enum(Some(&zero), &["B", "A"], "guard").unwrap_err(),
        "guard must be one of B, A"
    );
    let keyed: JsObjectMap<JsNumberInput> =
        js_from_str(r#"{"\ue000":1,"\ud800\udc00":2}"#).unwrap();
    require_exact_keys(&keyed, &["\u{e000}", "\u{10000}"], "guard").unwrap();
    let extra: JsObjectMap<JsNumberInput> = js_from_str(r#"{"extra":1}"#).unwrap();
    assert_eq!(
        require_exact_keys(&extra, &["\u{e000}", "\u{10000}"], "guard").unwrap_err(),
        "guard must contain exactly \u{10000}, \u{e000}",
        "WOO-GUARD native UTF16 key order"
    );

    let lone = js_from_str::<JsNumberInput>(r#""\ud800""#).unwrap_err();
    assert!(
        lone.to_string()
            .contains("invalid utf-16: lone surrogate found"),
        "WOO-GUARD native typed String projection refusal"
    );
    assert!(
        js_from_str::<String>(r#""\ud800""#).is_err(),
        "WOO-GUARD native returned string cannot materialize"
    );
    let lone_key = js_from_str::<JsNumberInput>(r#"{"\ud800":1}"#).unwrap_err();
    assert!(
        lone_key
            .to_string()
            .contains("invalid utf-16: lone surrogate found"),
        "WOO-GUARD native typed key projection refusal"
    );
    assert!(
        js_from_str::<JsObjectMap<JsNumberInput>>(r#"{"\ud800":1}"#).is_err(),
        "WOO-GUARD native exact-keys input cannot materialize"
    );
    let ascii = js_from_str::<JsNumberInput>(r#""\\ud800""#).unwrap();
    assert_eq!(require_string(Some(&ascii), "guard").unwrap(), r"\ud800");
    assert_eq!(
        require_enum(Some(&ascii), &[r"\ud800"], "guard").unwrap(),
        r"\ud800"
    );
    let paired = js_from_str::<JsNumberInput>(r#""\ud800\udc00""#).unwrap();
    assert_eq!(require_string(Some(&paired), "guard").unwrap(), "\u{10000}");

    for (script, groups) in [
        ("account-leverage-oracle.cjs", 6),
        ("pricing-oracle.cjs", 2),
    ] {
        let out = Command::new("node")
            .arg(format!("{}/scripts/{script}", env!("CARGO_MANIFEST_DIR")))
            .arg("--guard-audit")
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "WOO-GUARD source {script}: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        let value: Value = serde_json::from_slice(&out.stdout).unwrap();
        assert_eq!(
            value["groups"], groups,
            "WOO-GUARD independently fixed source groups"
        );
        if groups == 6 {
            assert_eq!(value["surrogateRows"], 4);
        }
    }
}
#[test]
fn account_choices_and_validation_order_are_native_before_source() {
    use safe_cex_venue_woo::v3_position_mode::decode_woo_v3_set_position_mode_response;
    let account: JsNumberInput=js_from_str(r#"{"source":"decoded-v3-account-info","reconciliationId":"audit-17","accountMode":"MARGIN","positionMode":"ONE_WAY"}"#).unwrap();
    let mut provenance = validate_account_provenance(Some(&account)).unwrap();
    assert_eq!(provenance.source, "decoded-v3-account-info");
    assert_eq!(provenance.reconciliation_id, "audit-17");
    assert_eq!(provenance.account_mode, "MARGIN");
    assert!(matches!(
        provenance.position_mode,
        WooV3AccountPositionMode::OneWay
    ));
    provenance.reconciliation_id = "owned-19".into();
    assert_eq!(provenance.reconciliation_id, "owned-19");
    let get_input = js_from_str(
        r#"{"symbol":"PERP_BTC_USDT","marginMode":"ISOLATED","positionMode":"HEDGE_MODE"}"#,
    )
    .unwrap();
    let mut get = build_woo_v3_get_leverage_request(&get_input).unwrap();
    assert!(matches!(get.request.method, WooV3HttpMethod::Get));
    assert!(matches!(get.request.path, WooV3Endpoint::FuturesLeverage));
    get.request
        .query
        .as_mut()
        .unwrap()
        .insert("symbol".into(), js_from_str(r#""PERP_ETH_USDT""#).unwrap());
    assert_eq!(
        get.target.symbol, "PERP_BTC_USDT",
        "WOO-ACCOUNT-CHOICE native GET query and target are independent"
    );
    assert!(matches!(
        get.target.margin_mode,
        WooV3FuturesMarginMode::Isolated
    ));
    assert!(matches!(
        get.target.position_mode,
        WooV3AccountPositionMode::HedgeMode
    ));
    let set_input=js_from_str(r#"{"symbol":"PERP_BTC_USDT","marginMode":"CROSS","leverage":31,"account":{"source":"decoded-v3-account-info","reconciliationId":"audit-17","accountMode":"MARGIN","positionMode":"ONE_WAY"}}"#).unwrap();
    let WooV3SetLeverageBuildResult::Ready {
        request,
        target,
        scope_evidence,
    } = build_woo_v3_set_leverage_request(&set_input).unwrap()
    else {
        panic!("WOO-ACCOUNT-CHOICE native ready SET");
    };
    assert!(matches!(request.method, WooV3HttpMethod::Put));
    assert!(matches!(request.path, WooV3Endpoint::FuturesLeverage));
    assert_eq!(target.symbol, "PERP_BTC_USDT");
    assert_eq!(
        request
            .body
            .as_ref()
            .unwrap()
            .get("symbol")
            .unwrap()
            .clone()
            .decode::<String>()
            .unwrap(),
        "PERP_BTC_USDT"
    );
    assert_eq!(
        request
            .body
            .as_ref()
            .unwrap()
            .get("leverage")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        31_f64.to_bits()
    );
    assert_eq!(
        scope_evidence,
        "official-cross-setter-ignores-positionMode-one-way-account-provenance"
    );
    for (margin, account_mode, expected) in [
        ("ISOLATED", "SPOT", "futures-margin-account-mode-not-proven"),
        (
            "ISOLATED",
            "MARGIN",
            "isolated-margin-outside-safe-cex-contract",
        ),
        (
            "CROSS",
            "MARGIN",
            "cross-hedge-resulting-leg-scope-requires-demo-evidence",
        ),
    ] {
        let input=js_from_str(&format!(r#"{{"symbol":"PERP_BTC_USDT","marginMode":"{margin}","leverage":31,"account":{{"source":"decoded-v3-account-info","reconciliationId":"audit-17","accountMode":"{account_mode}","positionMode":"HEDGE_MODE"}}}}"#)).unwrap();
        let WooV3SetLeverageBuildResult::Blocked { blocker, .. } =
            build_woo_v3_set_leverage_request(&input).unwrap()
        else {
            panic!("WOO-ACCOUNT-CHOICE native blocked SET")
        };
        assert_eq!(
            blocker, expected,
            "WOO-ACCOUNT-CHOICE native blocker precedence"
        );
    }
    let gateway = js_from_str(r#"{"nested":{"value":41}}"#).unwrap();
    let mut error =
        WooV3AccountConfigResponseError::new("audit-error".into(), &gateway, Some(-0.0));
    assert_eq!(
        error.name, "WooV3AccountConfigResponseError",
        "WOO-ACCOUNT-CHOICE native error name"
    );
    assert_eq!(error.message, "audit-error");
    assert_eq!(error.contract_status, "vendor-error-envelope-undocumented");
    assert_eq!(error.code.unwrap().to_bits(), (-0_f64).to_bits());
    let nested: JsObjectMap<JsNumberInput> = error
        .gateway_response
        .get("nested")
        .unwrap()
        .clone()
        .decode()
        .unwrap();
    assert_eq!(
        nested
            .get("value")
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits(),
        41_f64.to_bits(),
        "WOO-ACCOUNT-CHOICE native gateway nested payload"
    );
    error
        .gateway_response
        .insert("extra".into(), js_from_str("43").unwrap());
    assert!(
        !gateway.contains_key("extra"),
        "WOO-ACCOUNT-CHOICE native mutable independent error snapshot"
    );
    let success: JsNumberInput = js_from_str(r#"{"success":true,"timestamp":-0}"#).unwrap();
    assert_eq!(
        decode_success(Some(&success), "audit").unwrap().to_bits(),
        (-0_f64).to_bits()
    );
    for (raw, expected) in [
        (
            r#"{"success":false,"code":-1,"message":""}"#,
            "WOO V3 audit.code must be a safe integer of at least 0",
        ),
        (
            r#"{"success":true,"timestamp":-1,"extra":1}"#,
            "WOO V3 audit success response must contain exactly success, timestamp",
        ),
    ] {
        let input = js_from_str(raw).unwrap();
        let WooV3AccountWriteError::Validation(message) =
            decode_success(Some(&input), "audit").unwrap_err()
        else {
            panic!("WOO-ACCOUNT-CHOICE native validation classification")
        };
        assert_eq!(message, expected, "WOO-ACCOUNT-CHOICE native first refusal");
    }
    let read:JsNumberInput=js_from_str(r#"{"success":true,"timestamp":97,"data":{"symbol":"PERP_BTC_USDT","marginMode":"ISOLATED","positionMode":"HEDGE_MODE","details":[{"positionSide":"SHORT","leverage":29},{"positionSide":"LONG","leverage":17}]}}"#).unwrap();
    let WooV3LeverageReadback::HedgeMode {
        symbol,
        margin_mode,
        mut details,
        timestamp,
    } = decode_woo_v3_get_leverage_response(Some(&read), &get).unwrap()
    else {
        panic!("WOO-ACCOUNT-CHOICE native hedge readback")
    };
    assert_eq!(symbol, "PERP_BTC_USDT");
    assert!(matches!(margin_mode, WooV3FuturesMarginMode::Isolated));
    assert_eq!(
        details[0].position_side, "LONG",
        "WOO-ACCOUNT-CHOICE native LONG first"
    );
    assert_eq!(details[0].leverage.to_bits(), 17_f64.to_bits());
    assert_eq!(details[1].position_side, "SHORT");
    assert_eq!(details[1].leverage.to_bits(), 29_f64.to_bits());
    assert_eq!(timestamp.to_bits(), 97_f64.to_bits());
    details[0].leverage = 47.0;
    assert_eq!(details[0].leverage.to_bits(), 47_f64.to_bits());
    let captured_target:WooV3LeverageTarget=js_from_str(r#"{"symbol":"PERP_BTC_USDT","marginMode":"CROSS","positionMode":"ONE_WAY","leverage":31,"family":"source-extra","extra":89}"#).unwrap();
    let mutation =
        decode_woo_v3_set_leverage_response(Some(&success), &request, &captured_target).unwrap();
    assert!(mutation.accepted);
    assert_eq!(mutation.timestamp.to_bits(), (-0_f64).to_bits());
    let WooV3AccountMutationTarget::FuturesLeverage(actual) = mutation.target else {
        panic!("WOO-ACCOUNT-CHOICE native fixed leverage family")
    };
    assert_eq!(actual.symbol, "PERP_BTC_USDT");
    assert_eq!(actual.leverage.to_bits(), 31_f64.to_bits());
    assert!(matches!(actual.margin_mode, WooV3FuturesMarginMode::Cross));
    assert!(matches!(
        actual.position_mode,
        WooV3AccountPositionMode::OneWay
    ));
    assert_eq!(
        mutation.state_evidence,
        "request-target-only-response-does-not-echo-resulting-state"
    );
    let mode_request = WooV3RequestInput {
        method: WooV3HttpMethod::Put,
        path: WooV3Endpoint::FuturesPositionMode,
        query: None,
        body: None,
    };
    let mode = decode_woo_v3_set_position_mode_response(
        Some(&success),
        &mode_request,
        WooV3AccountPositionMode::HedgeMode,
    )
    .unwrap();
    assert!(mode.accepted);
    assert_eq!(mode.timestamp.to_bits(), (-0_f64).to_bits());
    assert!(
        matches!(
            mode.target,
            WooV3AccountMutationTarget::PositionMode {
                position_mode: WooV3AccountPositionMode::HedgeMode
            }
        ),
        "WOO-ACCOUNT-CHOICE native position-mode target"
    );
    assert_eq!(
        mode.state_evidence,
        "request-target-only-response-does-not-echo-resulting-state"
    );
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/account-leverage-oracle.cjs"
        ))
        .arg("--choice-audit")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let source: Value = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(
        source["groups"], 8,
        "WOO-ACCOUNT-CHOICE source floor ends at position-mode response"
    );
}
fn input(row: &Value, key: &str) -> Option<JsNumberInput> {
    row[key].as_str().map(|s| js_from_str(s).unwrap())
}
fn number(actual: f64, expected: &Value, label: &str) {
    assert_eq!(
        actual.to_bits(),
        expected["$number"]
            .as_str()
            .unwrap()
            .parse::<f64>()
            .unwrap()
            .to_bits(),
        "{label}"
    );
}
fn captured(actual: &JsNumberInput, expected: &Value) {
    if let Some(n) = expected.get("$number") {
        number(
            actual.clone().decode::<f64>().unwrap(),
            &serde_json::json!({"$number":n}),
            "ACCOUNT-LEVERAGE native captured number",
        );
        return;
    }
    match expected {
        Value::Null => assert!(actual.clone().decode::<()>().is_ok()),
        Value::Bool(v) => assert_eq!(
            actual.clone().decode::<bool>().unwrap(),
            *v,
            "ACCOUNT-LEVERAGE native captured bool"
        ),
        Value::String(v) => assert_eq!(
            actual.clone().decode::<String>().unwrap(),
            *v,
            "ACCOUNT-LEVERAGE native captured string"
        ),
        Value::Array(rows) => {
            let native = actual.clone().decode::<Vec<JsNumberInput>>().unwrap();
            assert_eq!(
                native.len(),
                rows.len(),
                "ACCOUNT-LEVERAGE native captured count"
            );
            for (v, e) in native.iter().zip(rows) {
                captured(v, e);
            }
        }
        Value::Object(rows) => {
            let native = actual
                .clone()
                .decode::<JsObjectMap<JsNumberInput>>()
                .unwrap();
            assert_eq!(
                native.len(),
                rows.len(),
                "ACCOUNT-LEVERAGE native captured keys count"
            );
            for (k, e) in rows {
                captured(native.get(k).unwrap(), e);
            }
        }
        _ => panic!("untagged source numeric expectation"),
    }
}
fn mode(actual: WooV3AccountPositionMode, expected: &Value) {
    let native = match expected.as_str().unwrap() {
        "ONE_WAY" => WooV3AccountPositionMode::OneWay,
        "HEDGE_MODE" => WooV3AccountPositionMode::HedgeMode,
        _ => panic!("source mode"),
    };
    assert_eq!(actual, native, "ACCOUNT-LEVERAGE native position mode");
}
fn margin(actual: WooV3FuturesMarginMode, expected: &Value) {
    assert_eq!(
        actual,
        match expected.as_str().unwrap() {
            "CROSS" => WooV3FuturesMarginMode::Cross,
            "ISOLATED" => WooV3FuturesMarginMode::Isolated,
            _ => panic!("source margin"),
        },
        "ACCOUNT-LEVERAGE native margin mode"
    );
}
fn provenance(actual: &WooV3AccountModeProvenance, e: &Value) {
    assert_eq!(actual.source, e["source"].as_str().unwrap());
    assert_eq!(
        actual.reconciliation_id,
        e["reconciliationId"].as_str().unwrap(),
        "ACCOUNT-LEVERAGE native reconciliation id"
    );
    assert_eq!(actual.account_mode, e["accountMode"].as_str().unwrap());
    mode(actual.position_mode, &e["positionMode"]);
}
fn get_target(actual: &WooV3GetLeverageInput, e: &Value) {
    assert_eq!(
        actual.symbol,
        e["symbol"].as_str().unwrap(),
        "ACCOUNT-LEVERAGE native target symbol"
    );
    margin(actual.margin_mode, &e["marginMode"]);
    mode(actual.position_mode, &e["positionMode"]);
}
fn set_target(actual: &WooV3LeverageTarget, e: &Value) {
    assert_eq!(actual.symbol, e["symbol"].as_str().unwrap());
    margin(actual.margin_mode, &e["marginMode"]);
    mode(actual.position_mode, &e["positionMode"]);
    number(
        actual.leverage,
        &e["leverage"],
        "ACCOUNT-LEVERAGE native target leverage",
    );
}
fn request(actual: &WooV3RequestInput, e: &Value) {
    assert_eq!(
        actual.method,
        match e["method"].as_str().unwrap() {
            "GET" => WooV3HttpMethod::Get,
            "PUT" => WooV3HttpMethod::Put,
            _ => panic!("source method"),
        },
        "ACCOUNT-LEVERAGE native request method"
    );
    assert_eq!(
        actual.path,
        WooV3Endpoint::FuturesLeverage,
        "ACCOUNT-LEVERAGE native request path"
    );
    assert_eq!(e["path"], "/v3/futures/leverage");
    for (native, key) in [(&actual.query, "query"), (&actual.body, "body")] {
        if let Some(expected) = e.get(key) {
            let native = native.as_ref().unwrap();
            let expected = expected.as_object().unwrap();
            assert_eq!(
                native.len(),
                expected.len(),
                "ACCOUNT-LEVERAGE native descriptor key count"
            );
            for (k, v) in expected {
                captured(native.get(k).unwrap(), v);
            }
        } else {
            assert!(
                native.is_none(),
                "ACCOUNT-LEVERAGE native absent descriptor map"
            );
        }
    }
}
fn get_build(actual: &WooV3GetLeverageRequest, e: &Value) {
    get_target(&actual.target, &e["target"]);
    request(&actual.request, &e["request"]);
}
fn set_build(actual: &WooV3SetLeverageBuildResult, e: &Value) {
    match (actual, e["ready"].as_bool().unwrap()) {
        (
            WooV3SetLeverageBuildResult::Ready {
                request: r,
                target,
                scope_evidence,
            },
            true,
        ) => {
            set_target(target, &e["target"]);
            request(r, &e["request"]);
            assert_eq!(
                scope_evidence,
                e["scopeEvidence"].as_str().unwrap(),
                "ACCOUNT-LEVERAGE native scope evidence"
            );
        }
        (WooV3SetLeverageBuildResult::Blocked { blocker, target }, false) => {
            set_target(target, &e["target"]);
            assert_eq!(
                blocker,
                e["blocker"].as_str().unwrap(),
                "ACCOUNT-LEVERAGE native blocker"
            );
        }
        _ => panic!("ACCOUNT-LEVERAGE native set build readiness"),
    }
}
fn readback(actual: &WooV3LeverageReadback, e: &Value) {
    let (symbol, margin_mode, details, time) = match actual {
        WooV3LeverageReadback::OneWay {
            symbol,
            margin_mode,
            details,
            timestamp,
        } => {
            assert_eq!(
                e["positionMode"], "ONE_WAY",
                "ACCOUNT-LEVERAGE native readback variant"
            );
            (symbol, *margin_mode, details.as_slice(), *timestamp)
        }
        WooV3LeverageReadback::HedgeMode {
            symbol,
            margin_mode,
            details,
            timestamp,
        } => {
            assert_eq!(
                e["positionMode"], "HEDGE_MODE",
                "ACCOUNT-LEVERAGE native readback variant"
            );
            (symbol, *margin_mode, details.as_slice(), *timestamp)
        }
    };
    assert_eq!(symbol, e["symbol"].as_str().unwrap());
    margin(margin_mode, &e["marginMode"]);
    number(
        time,
        &e["timestamp"],
        "ACCOUNT-LEVERAGE native readback timestamp",
    );
    let expected = e["details"].as_array().unwrap();
    assert_eq!(
        details.len(),
        expected.len(),
        "ACCOUNT-LEVERAGE native detail count"
    );
    for (v, e) in details.iter().zip(expected) {
        assert_eq!(
            v.position_side,
            e["positionSide"].as_str().unwrap(),
            "ACCOUNT-LEVERAGE native detail side"
        );
        number(
            v.leverage,
            &e["leverage"],
            "ACCOUNT-LEVERAGE native detail leverage",
        );
    }
}
fn gateway(actual: &WooV3AccountConfigResponseError, e: &Value) {
    assert_eq!(
        actual.name,
        e["name"].as_str().unwrap(),
        "ACCOUNT-LEVERAGE native error name"
    );
    assert_eq!(
        actual.message,
        e["message"].as_str().unwrap(),
        "ACCOUNT-LEVERAGE native error message"
    );
    assert_eq!(
        actual.contract_status,
        e["contractStatus"].as_str().unwrap()
    );
    if let Some(code) = e.get("code") {
        number(
            actual.code.unwrap(),
            code,
            "ACCOUNT-LEVERAGE native gateway code",
        );
    } else {
        assert!(actual.code.is_none());
    }
    let keys = e["gatewayKeys"]
        .as_array()
        .unwrap()
        .iter()
        .map(|x| x.as_str().unwrap())
        .collect::<Vec<_>>();
    assert_eq!(
        actual
            .gateway_response
            .keys()
            .map(String::as_str)
            .collect::<Vec<_>>(),
        keys,
        "ACCOUNT-LEVERAGE native gateway own-key order"
    );
    let fields = e["gatewayResponse"].as_object().unwrap();
    assert_eq!(actual.gateway_response.len(), fields.len());
    for (k, v) in fields {
        captured(actual.gateway_response.get(k).unwrap(), v);
    }
}
fn error(actual: WooV3AccountWriteError, e: &Value) {
    if e["name"] == "WooV3AccountConfigResponseError" {
        let WooV3AccountWriteError::Gateway(actual) = actual else {
            panic!("ACCOUNT-LEVERAGE native concrete gateway error")
        };
        gateway(&actual, e);
    } else {
        assert_eq!(e["name"], "Error");
        let WooV3AccountWriteError::Validation(message) = actual else {
            panic!("ACCOUNT-LEVERAGE native validation error")
        };
        assert_eq!(
            message,
            e["message"].as_str().unwrap(),
            "ACCOUNT-LEVERAGE native first error"
        );
    }
}
fn outcome<T>(actual: Result<T, String>, row: &Value, check: impl FnOnce(T, &Value)) {
    if let Some(e) = row.get("error") {
        assert_eq!(e["name"], "Error");
        assert_eq!(
            actual.err().unwrap(),
            e["message"].as_str().unwrap(),
            "ACCOUNT-LEVERAGE native helper first error"
        );
    } else {
        check(actual.ok().unwrap(), &row["ok"]);
    }
}
fn response<T>(
    actual: Result<T, WooV3AccountWriteError>,
    row: &Value,
    check: impl FnOnce(T, &Value),
) {
    if let Some(e) = row.get("error") {
        error(actual.err().unwrap(), e);
    } else {
        check(actual.ok().unwrap(), &row["ok"]);
    }
}
fn object_input(row: &Value) -> JsObjectMap<JsNumberInput> {
    input(row, "input").unwrap().decode().unwrap()
}
fn overrides(request: &mut WooV3RequestInput, row: &Value) {
    if let Some(o) = row.get("requestOverrides") {
        if let Some(method) = o.get("method") {
            request.method = if method == "PUT" {
                WooV3HttpMethod::Put
            } else {
                WooV3HttpMethod::Get
            };
        }
        if o.get("path").is_some() {
            request.path = WooV3Endpoint::FuturesPositionMode;
        }
    }
}
#[test]
fn decoded_native_declarations_and_fixed_metadata() {
    let data = source();
    for raw in ["null", "[]", "true", "3", "\"BAD\""] {
        assert!(js_from_str::<WooV3AccountPositionMode>(raw).is_err());
        assert!(js_from_str::<WooV3FuturesMarginMode>(raw).is_err());
    }
    let pos = [
        WooV3AccountPositionMode::OneWay,
        WooV3AccountPositionMode::HedgeMode,
    ];
    let margins = [
        WooV3FuturesMarginMode::Cross,
        WooV3FuturesMarginMode::Isolated,
    ];
    for i in 0..2 {
        let raw = data["enumValues"]["positionMode"][i].as_str().unwrap();
        let actual: WooV3AccountPositionMode =
            js_from_str(&serde_json::to_string(raw).unwrap()).unwrap();
        assert_eq!(actual, pos[i], "ACCOUNT-LEVERAGE native position mode");
        let raw = data["enumValues"]["marginMode"][i].as_str().unwrap();
        let actual: WooV3FuturesMarginMode =
            js_from_str(&serde_json::to_string(raw).unwrap()).unwrap();
        assert_eq!(actual, margins[i], "ACCOUNT-LEVERAGE native margin mode");
    }
    assert_eq!(data["declarations"].as_array().unwrap().len(), 6);
    let names = [
        "WooV3AccountModeProvenance",
        "WooV3SetLeverageInput",
        "WooV3GetLeverageInput",
        "WooV3GetLeverageRequest",
        "WooV3LeverageReadback",
        "WooV3LeverageReadback",
    ];
    for i in 0..6 {
        let row = &data["declarations"][i];
        assert_eq!(row["type"], names[i]);
        let raw = row["input"].as_str().unwrap();
        let e = &row["expected"];
        match names[i] {
            "WooV3AccountModeProvenance" => {
                provenance(&js_from_str::<WooV3AccountModeProvenance>(raw).unwrap(), e)
            }
            "WooV3SetLeverageInput" => {
                let actual: WooV3SetLeverageInput = js_from_str(raw).unwrap();
                assert_eq!(actual.symbol, e["symbol"].as_str().unwrap());
                margin(actual.margin_mode, &e["marginMode"]);
                number(
                    actual.leverage,
                    &e["leverage"],
                    "ACCOUNT-LEVERAGE native declaration leverage",
                );
                provenance(&actual.account, &e["account"]);
            }
            "WooV3GetLeverageInput" => {
                get_target(&js_from_str::<WooV3GetLeverageInput>(raw).unwrap(), e)
            }
            "WooV3GetLeverageRequest" => {
                get_build(&js_from_str::<WooV3GetLeverageRequest>(raw).unwrap(), e)
            }
            _ => readback(&js_from_str::<WooV3LeverageReadback>(raw).unwrap(), e),
        }
    }
    for raw in ["[]", "null", "{}"] {
        assert!(js_from_str::<WooV3GetLeverageInput>(raw).is_err());
        assert!(js_from_str::<WooV3SetLeverageInput>(raw).is_err());
        assert!(js_from_str::<WooV3LeverageReadback>(raw).is_err());
    }
    let e = &data["parity"];
    assert_eq!(e.as_object().unwrap().len(), 11);
    let p = WOO_V3_ACCOUNT_WRITE_PARITY;
    assert_eq!(
        p.request_and_response_fixtures,
        e["requestAndResponseFixtures"].as_bool().unwrap()
    );
    assert_eq!(
        p.runtime_imported,
        e["runtimeImported"].as_bool().unwrap(),
        "ACCOUNT-LEVERAGE native runtime imported"
    );
    assert_eq!(
        p.credentialed_demo_lifecycle,
        e["credentialedDemoLifecycle"].as_bool().unwrap()
    );
    assert_eq!(
        p.readback_required,
        e["readbackRequired"].as_bool().unwrap()
    );
    for (actual, key) in [
        (p.leverage_readback, "leverageReadback"),
        (p.default_margin_mode, "defaultMarginMode"),
        (p.cross_one_way_leverage, "crossOneWayLeverage"),
        (p.cross_hedge_leverage, "crossHedgeLeverage"),
        (p.isolated_leverage, "isolatedLeverage"),
        (p.position_mode_preconditions, "positionModePreconditions"),
        (p.mutation_response_state, "mutationResponseState"),
    ] {
        assert_eq!(
            actual,
            e[key].as_str().unwrap(),
            "ACCOUNT-LEVERAGE native metadata {key}"
        );
    }
}
#[test]
fn native_helpers_and_leverage_builders() {
    let data = source();
    assert_eq!(data["helpers"].as_array().unwrap().len(), 90);
    let names = [
        "requireRecord",
        "requireBoolean",
        "requireInteger",
        "requireString",
        "requireEnum",
    ];
    for family in 0..5 {
        for case in 0..18 {
            let row = &data["helpers"][family * 18 + case];
            assert_eq!(row["name"], names[family]);
            let v = input(row, "input");
            match names[family] {
                "requireRecord" => outcome(require_record(v.as_ref(), "probe"), row, |v, e| {
                    assert_eq!(v.len(), e.as_object().unwrap().len());
                    for (k, e) in e.as_object().unwrap() {
                        captured(v.get(k).unwrap(), e);
                    }
                }),
                "requireBoolean" => outcome(require_boolean(v.as_ref(), "probe"), row, |v, e| {
                    assert_eq!(v, e.as_bool().unwrap())
                }),
                "requireInteger" => {
                    outcome(require_integer(v.as_ref(), "probe", 0.0), row, |v, e| {
                        number(v, e, "ACCOUNT-LEVERAGE native integer")
                    })
                }
                "requireString" => outcome(require_string(v.as_ref(), "probe"), row, |v, e| {
                    assert_eq!(v, e.as_str().unwrap())
                }),
                _ => outcome(
                    require_enum(v.as_ref(), &["ONE_WAY", "HEDGE_MODE"], "probe"),
                    row,
                    |v, e| assert_eq!(v, e.as_str().unwrap()),
                ),
            }
        }
    }
    assert_eq!(data["exactKeys"].as_array().unwrap().len(), 5);
    for row in data["exactKeys"].as_array().unwrap() {
        let keys = row["keys"]
            .as_array()
            .unwrap()
            .iter()
            .map(|x| x.as_str().unwrap())
            .collect::<Vec<_>>();
        outcome(
            require_exact_keys(&object_input(row), &keys, "probe"),
            row,
            |_, e| assert_eq!(e, true),
        );
    }
    assert_eq!(data["symbolCases"].as_array().unwrap().len(), 8);
    for row in data["symbolCases"].as_array().unwrap() {
        let symbol: String = input(row, "input").unwrap().decode().unwrap();
        outcome(require_perpetual_symbol(&symbol), row, |v, e| {
            assert_eq!(v, e.as_str().unwrap())
        });
    }
    assert_eq!(data["provenance"].as_array().unwrap().len(), 9);
    for row in data["provenance"].as_array().unwrap() {
        let v = input(row, "input");
        outcome(validate_account_provenance(v.as_ref()), row, |v, e| {
            provenance(&v, e)
        });
    }
    assert_eq!(data["getBuilds"].as_array().unwrap().len(), 6);
    for row in data["getBuilds"].as_array().unwrap() {
        outcome(
            build_woo_v3_get_leverage_request(&object_input(row)),
            row,
            |v, e| get_build(&v, e),
        );
    }
    assert_eq!(data["setBuilds"].as_array().unwrap().len(), 13);
    for row in data["setBuilds"].as_array().unwrap() {
        outcome(
            build_woo_v3_set_leverage_request(&object_input(row)),
            row,
            |v, e| set_build(&v, e),
        );
    }
    // The source regex accepts/coerces a single-element array outside its declared string symbol type.
    let row = &data["strictSymbol"];
    assert!(row["source"]["target"]["symbol"].is_array());
    assert_eq!(
        build_woo_v3_get_leverage_request(&object_input(row))
            .err()
            .unwrap(),
        "WOO V3 symbol must use PERP_BASE_QUOTE form",
        "ACCOUNT-LEVERAGE chosen string-only symbol refusal"
    );
}
#[test]
fn native_readback_and_concrete_gateway_errors() {
    let data = source();
    let one = build_woo_v3_get_leverage_request(&object_input(&data["getBuilds"][0])).unwrap();
    let hedge = build_woo_v3_get_leverage_request(&object_input(&data["getBuilds"][1])).unwrap();
    assert_eq!(data["getResponses"].as_array().unwrap().len(), 29);
    for row in data["getResponses"].as_array().unwrap() {
        let mut build = if row["build"] == "one" {
            one.clone()
        } else {
            hedge.clone()
        };
        overrides(&mut build.request, row);
        let v = input(row, "input");
        response(
            decode_woo_v3_get_leverage_response(v.as_ref(), &build),
            row,
            |v, e| readback(&v, e),
        );
    }
    let e = &data["constructor"];
    let mut gateway_input:JsObjectMap<JsNumberInput>=js_from_str(r#"{"code":-0,"success":false,"message":"constructor message","nested":{"time":1e999,"zero":-0}}"#).unwrap();
    let native =
        WooV3AccountConfigResponseError::new("direct error".into(), &gateway_input, Some(-0.0));
    gateway(&native, e);
    gateway_input.insert("message".into(), js_from_str("\"later outer\"").unwrap());
    gateway_input.insert(
        "nested".into(),
        js_from_str(r#"{"time":99,"zero":-0}"#).unwrap(),
    );
    gateway(&native, e); // Native ownership is a full retained snapshot; source keeps nested aliases.
}
#[test]
fn native_mutation_acceptance_and_response_order() {
    let data = source();
    assert_eq!(data["successes"].as_array().unwrap().len(), 12);
    for row in data["successes"].as_array().unwrap() {
        let v = input(row, "input");
        response(
            decode_success(v.as_ref(), "set futures leverage"),
            row,
            |v, e| number(v, e, "ACCOUNT-LEVERAGE native success timestamp"),
        );
    }
    let WooV3SetLeverageBuildResult::Ready {
        request, target, ..
    } = build_woo_v3_set_leverage_request(&object_input(&data["setBuilds"][0])).unwrap()
    else {
        panic!("source ready build")
    };
    assert_eq!(data["mutations"].as_array().unwrap().len(), 14);
    for row in data["mutations"].as_array().unwrap() {
        let v = input(row, "input");
        let mut r = request.clone();
        overrides(&mut r, row);
        response(
            decode_woo_v3_set_leverage_response(v.as_ref(), &r, &target),
            row,
            |v, e| {
                assert_eq!(
                    v.accepted,
                    e["accepted"].as_bool().unwrap(),
                    "ACCOUNT-LEVERAGE native accepted"
                );
                number(
                    v.timestamp,
                    &e["timestamp"],
                    "ACCOUNT-LEVERAGE native mutation timestamp",
                );
                assert_eq!(
                    v.state_evidence,
                    e["stateEvidence"].as_str().unwrap(),
                    "ACCOUNT-LEVERAGE native state evidence"
                );
                let WooV3AccountMutationTarget::FuturesLeverage(t) = v.target else {
                    panic!("ACCOUNT-LEVERAGE native mutation family")
                };
                assert_eq!(e["target"]["family"], "futures-leverage");
                set_target(&t, &e["target"]);
            },
        );
    }
}
