use safe_cex_num::js_number_string;
use safe_cex_venue_bitget::order_lifecycle::OrderLifecycleCaches;
use serde_json::{Value, json};
use std::cell::Cell;
fn number(s: &str) -> f64 {
    match s {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        s => s.parse().unwrap(),
    }
}
fn entries(map: &indexmap::IndexMap<String, f64>) -> Value {
    Value::Array(
        map.iter()
            .map(|(k, n)| {
                json!([
                    k,
                    if *n == 0.0 && n.is_sign_negative() {
                        "-0".to_owned()
                    } else {
                        js_number_string(*n)
                    }
                ])
            })
            .collect(),
    )
}
fn check(oracle: &Value) {
    for case in oracle["cases"].as_array().unwrap() {
        let load = |kind: &str| {
            case["initial"][kind]
                .as_array()
                .unwrap()
                .iter()
                .map(|pair| {
                    (
                        pair[0].as_str().unwrap().into(),
                        number(pair[1].as_str().unwrap()),
                    )
                })
                .collect()
        };
        let mut caches = OrderLifecycleCaches {
            terminal_order_tombstones: load("terminal"),
            recent_ws_open_orders: load("open"),
        };
        let calls = Cell::new(0);
        for (i, op) in case["operations"].as_array().unwrap().iter().enumerate() {
            let now = number(op["now"].as_str().unwrap());
            let clock = || {
                calls.set(calls.get() + 1);
                now
            };
            match op["kind"].as_str().unwrap() {
                "prune" => caches.prune_order_lifecycle_caches(now),
                "terminal" => caches.mark_order_as_terminal(op["id"].as_str().unwrap(), clock),
                "open" => caches.mark_order_as_open_from_ws(op["id"].as_str().unwrap(), clock),
                kind => panic!("unknown operation {kind}"),
            }
            assert_eq!(
                json!({"terminal":entries(&caches.terminal_order_tombstones),"open":entries(&caches.recent_ws_open_orders),"clockCalls":calls.get()}),
                case["states"][i],
                "order lifecycle {} operation {i}",
                case["name"]
            );
        }
    }
    for witness in oracle["utf16"].as_array().unwrap() {
        assert!(
            serde_json::from_str::<String>(witness["input"].as_str().unwrap()).is_err(),
            "order cache UTF8 key ingress refusal"
        );
        assert!(
            witness["terminal"].as_str().unwrap().contains("\\ud"),
            "source Map retains lone surrogate key"
        );
    }
}
#[test]
fn frozen_order_lifecycle() {
    check(&serde_json::from_str(include_str!("order-lifecycle-oracle.json")).unwrap());
}
#[test]
#[ignore = "requires live Node and TypeScript"]
fn live_order_lifecycle() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-lifecycle-capture.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle = serde_json::from_slice(&output.stdout).unwrap();
    check(&oracle);
    assert_eq!(
        oracle,
        serde_json::from_str::<Value>(include_str!("order-lifecycle-oracle.json")).unwrap(),
        "order lifecycle source drift"
    );
}
