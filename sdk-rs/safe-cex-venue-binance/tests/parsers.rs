use safe_cex_num::js_number_string;
use safe_cex_venue_binance::{closed_pnl, order_history};
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn history_numeric_parsers_match_live_typescript() {
    let out = std::process::Command::new("node")
        .arg(format!(
            "{}/scripts/parsers.mjs",
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
    assert_eq!(cases.len(), 3534, "SOURCE-CARDINALITY parsers total");
    let mut completed_comparisons = 0;
    for (index, case) in cases.iter().enumerate() {
        let input = case.get("input");
        let result = match case["name"].as_str().unwrap() {
            "order-history.toNumber" => order_history::to_number(input).map(Some),
            "order-history.toOptionalNumber" => order_history::to_optional_number(input),
            "closed-pnl.toNumber" => closed_pnl::to_number(input).map(Some),
            name => panic!("unexpected oracle function {name}"),
        };
        let actual = match result {
            Err(_) => json!({"error":true}),
            Ok(None) => json!({"absent":true}),
            Ok(Some(n)) => {
                json!({"value":js_number_string(n),"negativeZero":n == 0.0 && n.is_sign_negative()})
            }
        };
        assert_eq!(actual, case["expected"], "case {index}: {case}");
        completed_comparisons += 1;
    }
    assert_eq!(
        completed_comparisons, 3534,
        "NATIVE-CARDINALITY parsers completed comparisons"
    );
    eprintln!("{} live numeric parser comparisons", cases.len());
}

#[test]
fn coercion_errors_remain_recoverable_and_zero_sign_survives() {
    let bad = json!({"toString":null});
    assert!(order_history::to_number(Some(&bad)).is_err());
    assert!(closed_pnl::to_number(Some(&bad)).is_err());
    assert!(order_history::to_optional_number(Some(&bad)).is_err());
    let zero = json!("-0tail");
    assert!(
        order_history::to_number(Some(&zero))
            .unwrap()
            .is_sign_negative()
    );
    assert_eq!(
        order_history::to_optional_number(Some(&zero)).unwrap(),
        None
    );
}
