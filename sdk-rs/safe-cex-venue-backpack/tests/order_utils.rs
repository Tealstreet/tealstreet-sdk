use safe_cex_types::{OrderStatus, OrderTimeInForce};
use safe_cex_venue_backpack::order_utils::{
    map_backpack_time_in_force, map_order_status, map_time_in_force,
};
use serde_json::Value;
use std::process::Command;

fn tif(value: &str) -> OrderTimeInForce {
    match value {
        "GoodTillCancel" => OrderTimeInForce::GoodTillCancel,
        "ImmediateOrCancel" => OrderTimeInForce::ImmediateOrCancel,
        "FillOrKill" => OrderTimeInForce::FillOrKill,
        "PostOnly" => OrderTimeInForce::PostOnly,
        _ => panic!("unknown actual source time-in-force: {value}"),
    }
}
#[test]
fn actual_typescript_order_enum_mappings() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order_utils_oracle.cjs"
        ))
        .output()
        .expect("one actual TypeScript utility corpus");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    for key in ["node", "arch", "platform"] {
        assert!(!corpus["runtime"][key].as_str().unwrap().is_empty());
    }
    eprintln!("order utility runtime: {}", corpus["runtime"]);
    for (key, count) in [("status", 13), ("tif", 5), ("reverse", 12), ("boundary", 2)] {
        assert_eq!(
            corpus[key].as_array().unwrap().len(),
            count,
            "ORDER-UTILS source count {key}"
        );
    }
    let mut compared = [0; 4];
    for (i, case) in corpus["status"].as_array().unwrap().iter().enumerate() {
        let expected = match case["result"].as_str().unwrap() {
            "open" => OrderStatus::Open,
            "closed" => OrderStatus::Closed,
            "canceled" => OrderStatus::Canceled,
            value => panic!("unexpected source status {value}"),
        };
        assert_eq!(
            map_order_status(case["input"].as_str().unwrap()),
            expected,
            "mapOrderStatus native enum case {i}"
        );
        compared[0] += 1;
    }
    for (i, case) in corpus["tif"].as_array().unwrap().iter().enumerate() {
        let input = case["input"].as_str().map(tif);
        assert_eq!(
            map_time_in_force(input),
            case["result"].as_str().unwrap(),
            "mapTimeInForce native wire literal case {i}"
        );
        compared[1] += 1;
    }
    for (i, case) in corpus["reverse"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            map_backpack_time_in_force(case["input"].as_str().unwrap()),
            tif(case["result"].as_str().unwrap()),
            "mapBackpackTimeInForce native enum case {i}"
        );
        compared[2] += 1;
    }
    for (i, case) in corpus["boundary"].as_array().unwrap().iter().enumerate() {
        typed_utf16::assert_projection(
            case["raw"].as_str().unwrap(),
            &(safe_cex_num::numeric_ingress::js_from_str::<String>(case["raw"].as_str().unwrap()))
                .expect("typed surrogate record is retained"),
        );
        assert_eq!(
            case["status"], "open",
            "source surrogate status default case {i}"
        );
        assert_eq!(
            case["reverse"], "GoodTillCancel",
            "source surrogate tif default case {i}"
        );
        compared[3] += 1;
    }
    assert_eq!(
        compared,
        [13, 5, 12, 2],
        "ORDER-UTILS completed comparisons"
    );
}

#[path = "../../test-support/typed_utf16.rs"]
mod typed_utf16;
