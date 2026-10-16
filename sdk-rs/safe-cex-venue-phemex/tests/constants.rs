use safe_cex_num::{ProtoMember, TableHit};
use safe_cex_venue_phemex::constants::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;

fn oracle() -> Value {
    let result = Command::new("node")
        .arg(format!(
            "{}/tests/constants_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("execute live TypeScript oracle");
    assert!(
        result.status.success(),
        "live TypeScript oracle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).expect("oracle JSON")
}

fn check_lookup<T: Serialize>(name: &str, table: &[(&str, T)], expected: &Value) -> usize {
    let observations = expected[name].as_object().unwrap();
    assert!(
        observations.contains_key("__proto__"),
        "prototype observation retained: {name}"
    );
    assert_eq!(
        observations.len(),
        table.len() + 13,
        "lookup census: {name}"
    );
    let mut comparisons = 0;
    for (key, observed) in observations {
        let actual = match lookup(table, key) {
            TableHit::Mapped(value) => {
                let value = serde_json::to_value(value).unwrap();
                // BASE_WS_URL's nested plain object stringifies as [object Object].
                let (kind, text) = if let Some(s) = value.as_str() {
                    ("string", s.to_owned())
                } else {
                    ("object", "[object Object]".into())
                };
                json!({"type":kind,"text":text,"truthy":true,"nullish":false})
            }
            TableHit::Inherited(member) => json!({
                "type": if member == ProtoMember::Prototype { "object" } else { "function" },
                "text": member.js_string(), "truthy":true,"nullish":false
            }),
            TableHit::Missing => {
                json!({"type":"undefined","text":"undefined","truthy":false,"nullish":true})
            }
        };
        assert_eq!(&actual, observed, "table lookup {name}.{key}");
        comparisons += 1;
    }
    comparisons
}

#[test]
fn constants_match_actual_typescript() {
    let expected = oracle();
    assert_eq!(
        expected["ordered"].as_object().unwrap().len(),
        13,
        "PHEMEX-CARDINALITY source constant total"
    );
    let actual = json!({
        "BROKER_ID": BROKER_ID, "RECV_WINDOW": RECV_WINDOW, "BASE_URL": BASE_URL, "BASE_WS_URL": BASE_WS_URL,
        "ENDPOINTS": ENDPOINTS, "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY, "PUBLIC_ENDPOINTS": PUBLIC_ENDPOINTS,
        "ORDER_STATUS": ORDER_STATUS, "ORDER_TYPE": ORDER_TYPE, "ORDER_SIDE": ORDER_SIDE,
        "POSITION_SIDE": POSITION_SIDE, "TIME_IN_FORCE": TIME_IN_FORCE,
    });
    assert_eq!(
        actual.as_object().unwrap().len(),
        expected["ordered"].as_object().unwrap().len(),
        "constant export census"
    );
    for (name, value) in actual.as_object().unwrap() {
        if name == "RECV_WINDOW" {
            continue;
        }
        assert_eq!(value, &expected["ordered"][name], "ordered constant {name}");
    }
    assert_eq!(
        expected["recvWindow"]["type"], "number",
        "RECV_WINDOW typeof"
    );
    assert_eq!(
        format!("{:016x}", RECV_WINDOW.to_bits()),
        expected["recvWindow"]["bits"],
        "RECV_WINDOW bits"
    );
    let lookups = &expected["lookups"];
    let mut comparisons = 0;
    comparisons += check_lookup("BASE_URL", &BASE_URL, lookups);
    comparisons += check_lookup("BASE_WS_URL", &BASE_WS_URL, lookups);
    comparisons += check_lookup("ENDPOINTS", &ENDPOINTS, lookups);
    comparisons += check_lookup("ORDER_STATUS", &ORDER_STATUS, lookups);
    comparisons += check_lookup("ORDER_TYPE", &ORDER_TYPE, lookups);
    comparisons += check_lookup("ORDER_SIDE", &ORDER_SIDE, lookups);
    comparisons += check_lookup("POSITION_SIDE", &POSITION_SIDE, lookups);
    comparisons += check_lookup("TIME_IN_FORCE", &TIME_IN_FORCE, lookups);
    assert_eq!(
        comparisons, 144,
        "PHEMEX-CARDINALITY lookup comparison total"
    );
}
