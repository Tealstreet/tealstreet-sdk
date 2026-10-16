use safe_cex_num::{ProtoMember, TableHit, js_table_lookup};
use safe_cex_venue_bitunix::types::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::process::Command;

fn table<T: Serialize>(entries: &[(&str, T)]) -> Value {
    let inherited = [
        "constructor",
        "__defineGetter__",
        "__defineSetter__",
        "hasOwnProperty",
        "__lookupGetter__",
        "__lookupSetter__",
        "isPrototypeOf",
        "propertyIsEnumerable",
        "toString",
        "valueOf",
        "__proto__",
        "toLocaleString",
    ];
    let keys = entries
        .iter()
        .map(|(key, _)| *key)
        .chain(["", "unknown", "buy"])
        .chain(inherited);
    let probes: Vec<_> = keys.map(|key| {
        let observation = match js_table_lookup(entries, key) {
            TableHit::Mapped(value) => json!({"type":"string", "text":serde_json::to_value(value).unwrap().as_str().unwrap()}),
            TableHit::Inherited(member) => json!({"type": if member == ProtoMember::Prototype {"object"} else {"function"}, "text":member.js_string()}),
            TableHit::Missing => json!({"type":"undefined", "text":"undefined"}),
        };
        json!([key, observation])
    }).collect();
    json!({"entries":entries, "probes":probes})
}

#[test]
fn actual_typescript_constants_and_pre_serialization_lookup() {
    let result = Command::new("node")
        .arg(format!(
            "{}/tests/constants-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("execute actual TypeScript oracle");
    assert!(
        result.status.success(),
        "actual TypeScript oracle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&result.stderr).trim()
    );
    let oracle: Value = serde_json::from_slice(&result.stdout).unwrap();
    let native = json!({
        "RECV_WINDOW": {"type":"number", "bits":format!("{:016x}", RECV_WINDOW.to_bits())},
        "BASE_URL": table(BASE_URL), "BASE_WS_URL": BASE_WS_URL,
        "ENDPOINTS": table(ENDPOINTS), "PUBLIC_ENDPOINTS": PUBLIC_ENDPOINTS,
        "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "ORDER_TYPE": table(ORDER_TYPE), "ORDER_SIDE": table(ORDER_SIDE),
        "POSITION_SIDE": table(POSITION_SIDE), "TIME_IN_FORCE": table(TIME_IN_FORCE),
        "ORDER_TRIGGER_TYPE": table(ORDER_TRIGGER_TYPE),
    });
    assert_eq!(
        native
            .as_object()
            .unwrap()
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        oracle
            .as_object()
            .unwrap()
            .keys()
            .collect::<std::collections::BTreeSet<_>>(),
        "constant export census"
    );
    let mut compared_exports = 0;
    for (name, expected) in oracle.as_object().unwrap() {
        compared_exports += 1;
        assert_eq!(&native[name], expected, "constant witness {name}");
    }
    assert_eq!(
        compared_exports, 12,
        "constant actual export comparison count"
    );
}
