use safe_cex_venue_coinbase::starbase::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
#[path = "support/compare.rs"]
mod comparison;
#[path = "support/schema.rs"]
mod schema;
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/starbase.cjs"))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).unwrap()
    })
}
#[test]
fn starbase_error_object_declaration() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase.rs"),
            "CoinbaseStarbaseJsonRpcErrorObject"
        ),
        oracle()["schema"]
    );
}

#[test]
fn starbase_endpoint_table_and_resolver() {
    let endpoints = oracle()["endpoints"].as_array().unwrap();
    assert_eq!(endpoints.len(), 2, "HTTP and WebSocket endpoint count");
    let expected = [
        (
            "http",
            "https://drb.coinbase.com/api/v2",
            CoinbaseStarbaseTransportKind::Http,
        ),
        (
            "websocket",
            "wss://drb.coinbase.com/ws/api/v2",
            CoinbaseStarbaseTransportKind::Websocket,
        ),
    ];
    for (case, (expected_kind, expected_url, kind)) in endpoints.iter().zip(expected) {
        assert_eq!(
            case["kind"].as_str(),
            Some(expected_kind),
            "endpoint kind identity"
        );
        assert_eq!(
            case["url"].as_str(),
            Some(expected_url),
            "TS endpoint table value"
        );
        assert_eq!(
            resolve_coinbase_starbase_endpoint(kind),
            expected_url,
            "resolved endpoint for {}",
            case["kind"]
        );
    }
}
#[test]
fn starbase_method_ascii_grammar() {
    for case in oracle()["methods"].as_array().unwrap() {
        let actual = match require_coinbase_starbase_method(case.get("input")) {
            Ok(v) => json!({"ok":v}),
            Err(e) => json!({"error":e}),
        };
        assert_eq!(actual, case["result"], "{case}");
    }
}
#[test]
fn starbase_record_safe_id_and_rpc_error_validation() {
    for case in oracle()["records"].as_array().unwrap() {
        assert_eq!(
            json!(is_record(case.get("input"))),
            case["result"],
            "record {case}"
        );
    }
    for case in oracle()["ids"].as_array().unwrap() {
        assert_eq!(
            json!(is_safe_request_id(case.get("input"))),
            case["result"],
            "id {case}"
        );
    }
    for case in oracle()["errors"].as_array().unwrap() {
        comparison::compare(
            &json!(parse_rpc_error(case.get("input"))),
            &case["result"],
            &format!("error {case}"),
        );
    }
    let supplement = &oracle()["errorIdentitySupplement"];
    assert_eq!(supplement["label"], "field-identity-supplement");
    let parsed = parse_rpc_error(Some(&supplement["input"])).expect("fixture RPC error");
    assert_eq!(
        parsed.message, "fixture-message-identity",
        "RPC error message identity"
    );
    assert_eq!(
        parsed.data,
        Some(json!({"marker":"fixture-data-identity"})),
        "RPC error data identity"
    );
    comparison::compare(
        &json!(parsed),
        &supplement["result"]["ok"],
        "RPC error identity supplement",
    );
}

#[test]
fn starbase_auth_declaration_and_validation() {
    assert_eq!(
        schema::wire_schema(
            include_str!("../src/starbase.rs"),
            "CoinbaseStarbaseAuthResult"
        ),
        oracle()["authSchema"]
    );
    for case in oracle()["auth"].as_array().unwrap() {
        let actual = match validate_auth_result(case.get("input")) {
            Ok(()) => json!({"ok":true}),
            Err(e) => json!({"error":e}),
        };
        assert_eq!(actual, case["result"], "auth {case}");
    }
    let result = CoinbaseStarbaseAuthResult {
        access_token: "token".into(),
        expires_in: 60.0,
        scope: "".into(),
        token_type: CoinbaseStarbaseTokenType::Bearer,
    };
    assert_eq!(
        serde_json::to_value(result).unwrap(),
        json!({"access_token":"token","expires_in":60.0,"scope":"","token_type":"bearer"})
    );
}

#[test]
fn starbase_transport_and_heartbeat_declarations() {
    for (name, expected) in oracle()["declarations"].as_object().unwrap() {
        assert_eq!(
            schema::wire_schema(include_str!("../src/starbase.rs"), name),
            *expected,
            "{name}"
        );
    }
    for (index, kind) in [
        CoinbaseStarbaseTransportKind::Http,
        CoinbaseStarbaseTransportKind::Websocket,
    ]
    .into_iter()
    .enumerate()
    {
        for authorization in [None, Some("".to_owned()), Some("Bearer token".to_owned())] {
            let context = CoinbaseStarbaseTransportContext {
                endpoint: "endpoint".into(),
                kind,
                authorization: authorization.clone(),
            };
            let actual = serde_json::to_value(context).unwrap();
            let mut expected = json!({
                "endpoint": "endpoint",
                "kind": oracle()["declarations"]["CoinbaseStarbaseTransportKind"]["values"][index],
            });
            if let Some(authorization) = authorization {
                expected["authorization"] = json!(authorization);
            }
            assert_eq!(actual, expected, "complete transport context");
        }
    }
    assert_eq!(
        serde_json::to_value(CoinbaseStarbaseHeartbeatSignal::Heartbeat).unwrap(),
        oracle()["declarations"]["CoinbaseStarbaseHeartbeatSignal"]["values"][0]
    );
    assert_eq!(
        serde_json::to_value(CoinbaseStarbaseHeartbeatSignal::TestRequest).unwrap(),
        oracle()["declarations"]["CoinbaseStarbaseHeartbeatSignal"]["values"][1]
    );
    assert_eq!(
        serde_json::to_value(CoinbaseStarbaseTestResult { version: "".into() }).unwrap(),
        json!({"version":""})
    );
}
