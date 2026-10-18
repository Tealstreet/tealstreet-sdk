use safe_cex_venue_binance::order_logic::is_post_only_rejection_error;
use serde_json::{Value, json};
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn post_only_rejections_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/rejection.mjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "{}",
        String::from_utf8_lossy(&out.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&out.stdout).unwrap();
    assert_eq!(cases.len(), 5191, "SOURCE-CARDINALITY rejection total");
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        let actual = is_post_only_rejection_error(c.get("input")).map(|v| json!({"value":v}));
        let actual = actual.unwrap_or_else(|e| json!({"error":e.name}));
        assert_eq!(actual, c["expected"], "case {index}: {c}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 5191,
        "NATIVE-CARDINALITY rejection completed comparisons"
    );
    eprintln!("{} live rejection comparisons", cases.len());
}
