use safe_cex_num::numeric_ingress::{JsNumberInput, js_from_str};
use safe_cex_venue_woo::{
    v3_contracts::{WooV3Endpoint, WooV3HttpMethod},
    v3_ws_session::*,
};
use serde_json::Value;
use std::process::Command;
fn source() -> Value {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/ws-session-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "WS-SESSION source oracle: {}",
        String::from_utf8_lossy(&out.stderr)
    );
    serde_json::from_slice(&out.stdout).unwrap()
}
fn input(row: &Value) -> Option<JsNumberInput> {
    row["input"]
        .as_str()
        .map(|raw| serde_json::from_str(raw).unwrap())
}
fn number(expected: &Value) -> f64 {
    expected["$number"].as_str().unwrap().parse().unwrap()
}
fn same_number(actual: f64, expected: &Value, name: &str) {
    assert_eq!(actual.to_bits(), number(expected).to_bits(), "{name}");
}
fn error<T: std::fmt::Debug>(actual: Result<T, String>, expected: &Value) {
    assert_eq!(
        actual.unwrap_err(),
        expected["error"]["message"].as_str().unwrap(),
        "WS-SESSION native validation error"
    );
    assert_eq!(expected["error"]["name"], "Error");
}
#[test]
fn native_scalar_and_decimal_validators() {
    let data = source();
    assert_eq!(data["scalars"].as_array().unwrap().len(), 19);
    for index in 0..19 {
        let row = &data["scalars"][index];
        let value = input(row);
        let actual = require_boolean(value.as_ref(), "test.boolean");
        if row["boolean"].get("ok").is_some() {
            assert_eq!(
                actual.unwrap(),
                row["boolean"]["ok"].as_bool().unwrap(),
                "WS-SESSION native boolean"
            );
        } else {
            error(actual, &row["boolean"]);
        }
        for (key, min) in [("integer", 0.0), ("minimumOne", 1.0)] {
            let actual = require_integer(value.as_ref(), "test.integer", min);
            if row[key].get("ok").is_some() {
                same_number(
                    actual.unwrap(),
                    &row[key]["ok"],
                    "WS-SESSION native integer",
                );
            } else {
                error(actual, &row[key]);
            }
        }
    }
    assert_eq!(data["decimals"].as_array().unwrap().len(), 25);
    for index in 0..25 {
        let row = &data["decimals"][index];
        let value = input(row);
        for (key, actual) in [
            ("decimal", require_decimal(value.as_ref(), "test.decimal")),
            (
                "nonNegative",
                require_non_negative_decimal(value.as_ref(), "test.decimal"),
            ),
            (
                "positive",
                require_positive_decimal(value.as_ref(), "test.decimal"),
            ),
        ] {
            if row[key].get("ok").is_some() {
                assert_eq!(
                    actual.unwrap(),
                    row[key]["ok"].as_str().unwrap(),
                    "WS-SESSION native decimal text"
                );
            } else {
                error(actual, &row[key]);
            }
        }
    }
    let zero: JsNumberInput = serde_json::from_str("-0").unwrap();
    assert_eq!(
        require_integer(Some(&zero), "x", f64::NAN)
            .unwrap()
            .to_bits(),
        (-0.0f64).to_bits()
    );
}
#[test]
fn decoded_native_declarations_retain_numeric_values() {
    let key: WooV3ListenKey =
        js_from_str(r#"{"authKey":"native/key","expiredTime":37,"responseTimestamp":11}"#).unwrap();
    assert_eq!(
        key.auth_key, "native/key",
        "WS-SESSION native model auth key"
    );
    assert_eq!(key.expired_time, 37.0, "WS-SESSION native model expiry");
    assert_eq!(
        key.response_timestamp, 11.0,
        "WS-SESSION native model timestamp"
    );
    let key: WooV3ListenKey =
        js_from_str(r#"{"authKey":"numeric","expiredTime":1e999,"responseTimestamp":-0}"#).unwrap();
    assert_eq!(
        key.expired_time,
        f64::INFINITY,
        "WS-SESSION native unrestricted expiry"
    );
    assert_eq!(
        key.response_timestamp.to_bits(),
        (-0.0f64).to_bits(),
        "WS-SESSION native signed zero timestamp"
    );
    let ping: WooV3WsPing = js_from_str(r#"{"cmd":"PING","ts":-0}"#).unwrap();
    assert_eq!(ping.cmd, "PING", "WS-SESSION native model ping cmd");
    assert_eq!(
        ping.ts.to_bits(),
        (-0.0f64).to_bits(),
        "WS-SESSION native model ping ts"
    );
    let pong: WooV3WsPong = js_from_str(r#"{"cmd":"PONG","success":true,"time":1e999}"#).unwrap();
    assert_eq!(pong.cmd, "PONG", "WS-SESSION native model pong cmd");
    assert!(pong.success, "WS-SESSION native model pong success");
    assert_eq!(
        pong.time,
        f64::INFINITY,
        "WS-SESSION native model pong time"
    );
    for raw in [
        "[]",
        r#"["k",37,11]"#,
        r#"{"authKey":"k","expiredTime":37}"#,
    ] {
        assert!(
            js_from_str::<WooV3ListenKey>(raw).is_err(),
            "WS-SESSION strict key declaration {raw}"
        );
    }
    for raw in [
        "[]",
        r#"{"cmd":"PONG","ts":1}"#,
        r#"{"cmd":{"PING":null},"ts":1}"#,
        r#"{"cmd":"PING","ts":"1"}"#,
    ] {
        assert!(
            js_from_str::<WooV3WsPing>(raw).is_err(),
            "WS-SESSION strict ping declaration {raw}"
        );
    }
    for raw in [
        "[]",
        r#"{"cmd":"PONG","success":false,"time":1}"#,
        r#"{"cmd":"PING","success":true,"time":1}"#,
        r#"{"cmd":"PONG","success":"true","time":1}"#,
    ] {
        assert!(
            js_from_str::<WooV3WsPong>(raw).is_err(),
            "WS-SESSION strict pong declaration {raw}"
        );
    }
}
#[test]
fn native_listen_key_request_decode_and_url() {
    let data = source();
    let request = build_woo_v3_listen_key_request();
    assert_eq!(
        request.method,
        WooV3HttpMethod::Post,
        "WS-SESSION native request method"
    );
    assert_eq!(data["request"]["method"], "POST");
    assert_eq!(
        request.path,
        WooV3Endpoint::AccountListenKey,
        "WS-SESSION native request endpoint"
    );
    assert_eq!(data["request"]["path"], "/v3/account/listenKey");
    assert!(request.query.is_none());
    assert!(data["request"].get("query").is_none());
    assert_eq!(
        data["request"]["body"].as_object().unwrap().len(),
        1,
        "WS-SESSION source request body floor"
    );
    let body = request.body.unwrap();
    assert_eq!(body.len(), 1, "WS-SESSION native request body count");
    assert_eq!(data["request"]["body"]["type"], "WEBSOCKET");
    assert_eq!(
        body.get("type")
            .unwrap()
            .clone()
            .decode::<String>()
            .unwrap(),
        "WEBSOCKET",
        "WS-SESSION native request body type"
    );
    assert_eq!(data["listen"].as_array().unwrap().len(), 16);
    for index in 0..16 {
        let row = &data["listen"][index];
        let value = input(row);
        let actual = decode_woo_v3_listen_key(value.as_ref());
        if let Some(expected) = row.get("ok") {
            let actual = actual.unwrap();
            assert_eq!(
                actual.auth_key,
                expected["authKey"].as_str().unwrap(),
                "WS-SESSION native listen auth key"
            );
            same_number(
                actual.expired_time,
                &expected["expiredTime"],
                "WS-SESSION native listen expiry",
            );
            same_number(
                actual.response_timestamp,
                &expected["responseTimestamp"],
                "WS-SESSION native listen timestamp",
            );
        } else {
            error(actual, row);
        }
    }
    assert_eq!(data["urls"].as_array().unwrap().len(), 8);
    for index in 0..8 {
        let row = &data["urls"][index];
        let value = input(row);
        let actual = build_woo_v3_private_websocket_url(
            row["environment"].as_str().unwrap(),
            value.as_ref(),
        );
        if let Some(expected) = row.get("ok") {
            assert_eq!(
                actual.unwrap(),
                expected.as_str().unwrap(),
                "WS-SESSION native private URL"
            );
        } else {
            error(actual, row);
        }
    }
}
#[test]
fn native_heartbeat_commands_and_contract() {
    let data = source();
    for index in 0..19 {
        let row = &data["scalars"][index];
        let value = input(row);
        let actual = build_woo_v3_ws_ping(value.as_ref());
        let expected = &row["ping"];
        if let Some(expected) = expected.get("ok") {
            let actual = actual.unwrap();
            assert_eq!(
                actual.cmd,
                expected["cmd"].as_str().unwrap(),
                "WS-SESSION native ping cmd"
            );
            same_number(
                actual.ts,
                &expected["ts"],
                "WS-SESSION native ping timestamp",
            );
        } else {
            error(actual, expected);
        }
    }
    assert_eq!(data["pong"].as_array().unwrap().len(), 12);
    for index in 0..12 {
        let row = &data["pong"][index];
        let value = input(row);
        let actual = decode_woo_v3_ws_pong(value.as_ref());
        if let Some(expected) = row.get("ok") {
            let actual = actual.unwrap();
            assert_eq!(
                actual.cmd,
                expected["cmd"].as_str().unwrap(),
                "WS-SESSION native pong cmd"
            );
            assert_eq!(
                actual.success,
                expected["success"].as_bool().unwrap(),
                "WS-SESSION native pong success"
            );
            same_number(
                actual.time,
                &expected["time"],
                "WS-SESSION native pong time",
            );
        } else {
            error(actual, row);
        }
    }
    let expected = &data["heartbeat"];
    assert_eq!(
        expected.as_object().unwrap().len(),
        4,
        "WS-SESSION source heartbeat field floor"
    );
    let actual = WOO_V3_WS_HEARTBEAT;
    assert_eq!(
        actual.classification,
        expected["classification"].as_str().unwrap(),
        "WS-SESSION native heartbeat classification"
    );
    assert_eq!(
        actual.server_initiated_ping_documented,
        expected["serverInitiatedPingDocumented"].as_bool().unwrap(),
        "WS-SESSION native heartbeat server ping"
    );
    assert_eq!(
        actual.client_ping_command,
        expected["clientPingCommand"].as_str().unwrap(),
        "WS-SESSION native heartbeat ping"
    );
    assert_eq!(
        actual.server_pong_command,
        expected["serverPongCommand"].as_str().unwrap(),
        "WS-SESSION native heartbeat pong"
    );
}
