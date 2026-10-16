#[path = "support/cardinality.rs"]
mod cardinality;
use safe_cex_venue_okx::{closed_pnl, income, order_history, position_adapter};
use serde_json::Value;
#[test]
#[ignore = "requires isolated TypeScript compiler; mandatory venue gate"]
fn earlier_numeric_helpers_preserve_native_bits_before_json() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/numeric-helper-identity-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 140);
    cardinality::assert_groups(
        &cases,
        "numeric_helper_identity operations",
        |case| case["op"].as_str().unwrap().to_owned(),
        &[
            ("income", 28),
            ("history", 28),
            ("optional", 28),
            ("closed", 28),
            ("position", 28),
        ],
    );
    for case in cases {
        let input = &case["input"];
        let actual = match case["op"].as_str().unwrap() {
            "income" => Some(income::to_number(input).unwrap()),
            "history" => Some(order_history::to_number(input).unwrap()),
            "closed" => Some(closed_pnl::to_number(input).unwrap()),
            "optional" => order_history::to_optional_number(input).unwrap(),
            "position" => Some(position_adapter::num(input).unwrap()),
            _ => panic!("unknown helper"),
        };
        assert_eq!(
            actual.is_some(),
            case["present"].as_bool().unwrap(),
            "helper field presence {}",
            case
        );
        if let Some(n) = actual {
            let bits = if n.is_nan() {
                0x7ff8_0000_0000_0000
            } else {
                n.to_bits()
            };
            assert_eq!(
                format!("{bits:016x}"),
                case["bits"].as_str().unwrap(),
                "helper native bit identity {}",
                case
            );
        }
    }
}
