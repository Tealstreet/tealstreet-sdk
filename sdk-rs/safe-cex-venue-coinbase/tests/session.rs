use safe_cex_venue_coinbase::{starbase::CoinbaseStarbaseProtocolError, starbase_session::*};
use serde::Serialize;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/session.cjs"))
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
fn result<T: Serialize>(value: Result<T, CoinbaseStarbaseProtocolError>) -> Value {
    match value {
        Ok(v) => json!({"ok":v}),
        Err(e) => json!({"error":e}),
    }
}
#[test]
fn session_scope_and_protocol_error_shapes() {
    use CoinbaseStarbaseCancelOnDisconnectScope::{Account, Connection};
    assert_eq!(
        json!([Connection, Account]),
        oracle()["scopes"],
        "complete scope literal set"
    );
    for expected in oracle()["errors"].as_array().unwrap() {
        let error = CoinbaseStarbaseProtocolError::new(expected["message"].as_str().unwrap());
        assert_eq!(
            json!(error),
            json!({"name":expected["name"],"message":expected["message"]})
        );
        assert_eq!(error.to_string(), expected["display"].as_str().unwrap());
    }
}
#[test]
fn session_request_scope_omission_and_rejection() {
    for case in oracle()["builders"].as_array().unwrap() {
        let scope = case.get("input");
        let actual = match case["method"].as_str().unwrap() {
            "buildCoinbaseStarbaseEnableCancelOnDisconnect" => {
                build_coinbase_starbase_enable_cancel_on_disconnect(scope)
            }
            "buildCoinbaseStarbaseDisableCancelOnDisconnect" => {
                build_coinbase_starbase_disable_cancel_on_disconnect(scope)
            }
            "buildCoinbaseStarbaseGetCancelOnDisconnect" => {
                build_coinbase_starbase_get_cancel_on_disconnect(scope)
            }
            name => panic!("unknown TS builder {name}"),
        };
        if let Ok(request) = &actual {
            let params: serde_json::Map<String, Value> = request.params.clone().into();
            assert_eq!(
                json!(params),
                case["result"]["ok"]["params"],
                "outbound params {case}"
            );
        }
        assert_eq!(result(actual), case["result"], "{case}");
    }
}
#[test]
fn session_decoders_exact_keys_and_error_precedence() {
    for case in oracle()["mutations"].as_array().unwrap() {
        assert_eq!(
            result(
                decode_coinbase_starbase_cancel_on_disconnect_mutation_result(case.get("input"))
            ),
            case["result"],
            "mutation {case}"
        );
    }
    for case in oracle()["getters"].as_array().unwrap() {
        assert_eq!(
            result(decode_coinbase_starbase_get_cancel_on_disconnect_result(
                case.get("input")
            )),
            case["result"],
            "getter {case}"
        );
    }
}

#[path = "support/schema.rs"]
mod schema;
#[test]
fn session_declared_shapes_match_both_directions() {
    let names = [
        "CoinbaseStarbaseCancelOnDisconnectScope",
        "CoinbaseStarbaseSessionRequest",
        "CoinbaseStarbaseCancelOnDisconnectParams",
        "CoinbaseStarbaseCancelOnDisconnectResult",
    ];
    let expected = oracle()["schemas"].as_object().unwrap();
    assert_eq!(
        expected.len(),
        names.len(),
        "session type declaration inventory"
    );
    for name in names {
        assert_eq!(
            schema::wire_schema(include_str!("../src/starbase_session.rs"), name),
            expected[name],
            "session schema {name}"
        );
    }
}
