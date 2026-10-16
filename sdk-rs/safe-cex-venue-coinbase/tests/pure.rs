use safe_cex_venue_coinbase::utils::*;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};
fn oracle() -> &'static Value {
    static ORACLE: OnceLock<Value> = OnceLock::new();
    ORACLE.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/pure.cjs"))
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
fn usd_equivalent_aliases() {
    for case in oracle()["aliases"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        assert_eq!(
            json!(normalize_coinbase_usd_equivalent_product_id(id)),
            case["normal"],
            "normalize {id}"
        );
        assert_eq!(
            json!(get_coinbase_usd_equivalent_alias_product_id(id)),
            case["alias"],
            "alias {id}"
        );
    }
}
#[test]
fn usd_equivalent_market_precedence_and_prototype_correction() {
    let mut prototype_cases = 0;
    for case in oracle()["markets"].as_array().unwrap() {
        let id = case["id"].as_str().unwrap();
        let map = case["map"].as_object().unwrap();
        let actual = resolve_coinbase_usd_equivalent_market(id, map);
        if !map.contains_key(id)
            && matches!(
                id,
                "constructor" | "__proto__" | "toString" | "valueOf" | "hasOwnProperty"
            )
        {
            prototype_cases += 1;
            assert_eq!(
                actual, None,
                "typed own-key JSON boundary rejects inherited {id}"
            );
            if id == "__proto__" {
                assert_eq!(
                    case["result"],
                    json!({"kind":"value","value":{}}),
                    "TS prototype object"
                );
            } else {
                assert_eq!(case["result"]["kind"], "function", "TS inherited {id}");
            }
        } else {
            let actual = actual.map_or(
                json!({"kind":"undefined"}),
                |v| json!({"kind":"value","value":v}),
            );
            assert_eq!(actual, case["result"], "{case}");
        }
    }
    assert_eq!(prototype_cases, 40, "all five inherited keys pinned");
}
#[test]
fn order_formatting_values_and_error_logs() {
    for name in ["states", "types"] {
        for case in oracle()[name].as_array().unwrap() {
            let value = case["value"].as_str().unwrap();
            let mut logs = Vec::new();
            let mut log = |message: &str, raw: &str| logs.push(json!([message, raw]));
            let actual = if name == "states" {
                json!(format_order_state(value, &mut log))
            } else {
                json!(format_order_type(value, &mut log))
            };
            assert_eq!(actual, case["result"], "{name} {value}");
            assert_eq!(json!(logs), case["logs"], "{name} {value} console.error");
        }
    }
}
