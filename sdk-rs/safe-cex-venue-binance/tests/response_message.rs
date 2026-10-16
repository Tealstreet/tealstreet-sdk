use safe_cex_venue_binance::response_message::{
    get_binance_pm_response_message, get_binance_response_message,
};
use serde_json::{Value, json};
#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn response_messages_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/response-message.mjs",
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
    assert_eq!(
        cases.len(),
        11828,
        "SOURCE-CARDINALITY response_message total"
    );
    let mut completed_comparisons = 0;
    for (index, c) in cases.iter().enumerate() {
        let f = if c["variant"] == "binance" {
            get_binance_response_message
        } else {
            get_binance_pm_response_message
        };
        let actual = match f(c.get("input")) {
            Ok(s) => json!({"value":s}),
            Err(e) => json!({"error":e.name}),
        };
        assert_eq!(actual, c["expected"], "case {index}: {c}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 11828,
        "NATIVE-CARDINALITY response_message completed comparisons"
    );
    eprintln!("{} live response-message comparisons", cases.len());
}
