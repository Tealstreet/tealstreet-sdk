use safe_cex_num::{TableHit, js_table_lookup};
use safe_cex_venue_bingx::constants::*;
use serde::Serialize;
use serde_json::{Value, json};
use std::{process::Command, sync::OnceLock};

fn oracle() -> &'static Value {
    static CORPUS: OnceLock<Value> = OnceLock::new();
    CORPUS.get_or_init(|| {
        let output = Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/constants_oracle.cjs"
            ))
            .output()
            .expect("run actual TypeScript constants");
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        serde_json::from_slice(&output.stdout).expect("oracle JSON")
    })
}

#[test]
fn constants_match_actual_typescript() {
    let expected = oracle();
    let actual = json!({
        "RECV_WINDOW": RECV_WINDOW, "BASE_URL": BASE_URL, "BASE_WS_URL": BASE_WS_URL,
        "ENDPOINTS": ENDPOINTS, "GRAFANA_ENDPOINTS_EVERY": GRAFANA_ENDPOINTS_EVERY,
        "GRAFANA_ENDPOINTS_PERIODIC": GRAFANA_ENDPOINTS_PERIODIC,
        "ORDER_TYPE": ORDER_TYPE, "ORDER_SIDE": ORDER_SIDE, "POSITION_SIDE": POSITION_SIDE,
        "TIME_IN_FORCE": TIME_IN_FORCE, "ORDER_STATUS": ORDER_STATUS,
        "ORDER_TRIGGER_TYPE": ORDER_TRIGGER_TYPE,
    });
    assert_eq!(
        actual.as_object().unwrap().len(),
        expected["ordered"].as_object().unwrap().len(),
        "export census"
    );
    // serde's integer and floating JSON representations are not interchangeable.
    assert_eq!(
        RECV_WINDOW.to_bits(),
        u64::from_str_radix(expected["recvBits"].as_str().unwrap(), 16).unwrap(),
        "RECV_WINDOW native bits"
    );
    assert_eq!(expected["recvType"], "number", "RECV_WINDOW native typeof");
    for (name, value) in actual.as_object().unwrap() {
        if name != "RECV_WINDOW" {
            assert_eq!(value, &expected["ordered"][name], "ordered constant {name}");
        }
    }
}

fn check_table<T: Serialize>(name: &str, table: &[(&str, T)], expected: &Value) {
    let probes = expected["inherited"][name].as_array().unwrap();
    assert_eq!(
        probes.len(),
        15,
        "BINGX-CARDINALITY inherited probes {name}"
    );
    let mut compared = 0;
    for probe in probes {
        let key = probe["key"].as_str().unwrap();
        let hit = js_table_lookup(table, key);
        let (kind, text) = match hit {
            TableHit::Inherited(member) => (
                if member == safe_cex_num::ProtoMember::Prototype {
                    "object"
                } else {
                    "function"
                },
                member.js_string().to_owned(),
            ),
            TableHit::Missing => ("undefined", "undefined".into()),
            TableHit::Mapped(value) => (
                "string",
                serde_json::to_value(value)
                    .unwrap()
                    .as_str()
                    .unwrap()
                    .into(),
            ),
        };
        assert_eq!(
            json!({"key":key,"type":kind,"text":text,"present":hit.is_present()}),
            *probe,
            "inherited lookup {name}/{key}"
        );
        compared += 1;
    }
    assert_eq!(
        compared, 15,
        "BINGX-CARDINALITY inherited comparisons {name}"
    );
    // Object spread replaces an existing inverse value without moving its key.
    let mut inverse: Vec<(Value, &str)> = Vec::new();
    for (key, value) in table {
        let value = serde_json::to_value(value).unwrap();
        if let Some(entry) = inverse.iter_mut().find(|(v, _)| *v == value) {
            entry.1 = key;
        } else {
            inverse.push((value, key));
        }
    }
    assert_eq!(
        serde_json::to_value(inverse).unwrap(),
        expected["inverse"][name],
        "inverse order {name}"
    );
}

#[test]
fn mapping_lookup_and_inverse_match_actual_typescript() {
    let expected = oracle();
    check_table("ORDER_TYPE", &ORDER_TYPE, expected);
    check_table("ORDER_SIDE", &ORDER_SIDE, expected);
    check_table("POSITION_SIDE", &POSITION_SIDE, expected);
    check_table("TIME_IN_FORCE", &TIME_IN_FORCE, expected);
    check_table("ORDER_STATUS", &ORDER_STATUS, expected);
    check_table("ORDER_TRIGGER_TYPE", &ORDER_TRIGGER_TYPE, expected);
}
