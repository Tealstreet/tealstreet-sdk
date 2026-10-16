use safe_cex_types::OrderType;
use safe_cex_venue_okx::exchange::{get_algo_order_placement_priority, is_algo_order};
use serde_json::Value;

#[test]
#[ignore = "requires Node and isolated TypeScript compiler; mandatory venue gate"]
fn declared_order_type_policy_matches_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/order-policy-oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(capture["capture"]["architecture"].is_string());
    assert!(capture["capture"]["nodeMajor"].is_number());
    let cases = capture["cases"].as_array().unwrap();
    assert_eq!(cases.len(), 6, "ORDER-POLICY native source total");
    let inventory = [
        OrderType::Market,
        OrderType::Limit,
        OrderType::Conditional,
        OrderType::StopLoss,
        OrderType::TakeProfit,
        OrderType::TrailingStopLoss,
    ];
    let mut predicate_comparisons = 0;
    let mut priority_comparisons = 0;
    for index in 0..6 {
        let kind = inventory[index];
        assert_eq!(
            serde_json::to_value(kind).unwrap(),
            cases[index]["type"],
            "ORDER-POLICY native enum identity {index}"
        );
        assert_eq!(
            is_algo_order(kind),
            cases[index]["algo"].as_bool().unwrap(),
            "ORDER-POLICY native algo identity {index}"
        );
        predicate_comparisons += 1;
        assert_eq!(
            u64::from(get_algo_order_placement_priority(kind)),
            cases[index]["priority"].as_u64().unwrap(),
            "ORDER-POLICY native priority identity {index}"
        );
        priority_comparisons += 1;
    }
    assert_eq!(
        predicate_comparisons, 6,
        "ORDER-POLICY predicate executions"
    );
    assert_eq!(priority_comparisons, 6, "ORDER-POLICY priority executions");
}
