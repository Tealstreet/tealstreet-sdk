use safe_cex_venue_woo::constants::*;
use serde_json::{Value, json};

#[test]
fn legacy_tables_match_live_typescript_values_and_iteration_order() {
    let expected: Value = serde_json::from_str(include_str!("fixtures/constants.json")).unwrap();
    let actual = json!({
        "RECV_WINDOW": RECV_WINDOW,
        "BROKER_ID": BROKER_ID,
        "TESTNET_BROKER_ID": TESTNET_BROKER_ID,
        "BASE_URL": BASE_URL,
        "BASE_WS_URL": BASE_WS_URL,
        "ENDPOINTS": ENDPOINTS,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "ORDER_TYPE": ORDER_TYPE,
        "ORDER_SIDE": ORDER_SIDE,
        "ORDER_TRIGGER_TYPE": ORDER_TRIGGER_TYPE,
    });
    assert_eq!(actual, expected["ordered"]);
}

#[test]
fn own_entry_templates_exclude_inherited_runtime_keys() {
    let expected: Value = serde_json::from_str(include_str!("fixtures/constants.json")).unwrap();
    for key in [
        "constructor",
        "__proto__",
        "toString",
        "valueOf",
        "hasOwnProperty",
    ] {
        assert!(!ORDER_TYPE.iter().any(|(k, _)| *k == key));
        assert!(!ORDER_SIDE.iter().any(|(k, _)| *k == key));
        assert!(!ORDER_TRIGGER_TYPE.iter().any(|(k, _)| *k == key));
        for table in ["ORDER_TYPE", "ORDER_SIDE", "ORDER_TRIGGER_TYPE"] {
            assert_eq!(
                expected["prototypeLookups"][table][key],
                if key == "__proto__" {
                    "object"
                } else {
                    "function"
                }
            );
        }
    }
}
