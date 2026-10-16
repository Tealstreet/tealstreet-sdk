use safe_cex_venue_orderly::errors::{
    get_orderly_order_error_log_options, is_expected_orderly_order_rejection,
};
use serde_json::Value;
use std::process::Command;

#[test]
fn actual_typescript_order_error_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/errors-oracle.cjs"
        ))
        .output()
        .expect("one Node process for order-error corpus");
    assert!(
        output.status.success(),
        "actual source oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["corpus"].as_array().unwrap().len(),
        175,
        "ORDERLY-CARDINALITY errors/corpus source total"
    );
    eprintln!("order-error runtime: {}", corpus["runtime"]);
    let mut compared_corpus = 0;
    for (index, row) in corpus["corpus"].as_array().unwrap().iter().enumerate() {
        let input = row["input"].as_str();
        assert_eq!(
            is_expected_orderly_order_rejection(input),
            row["expected"].as_bool().unwrap(),
            "order-rejection/classification/{index}"
        );
        let options = get_orderly_order_error_log_options(input);
        if row["options"].is_null() {
            assert!(options.is_none(), "order-log/absent/{index}");
        } else {
            let native = options.expect("source log options present");
            assert_eq!(native.len(), 1, "order-log/native-key-count/{index}");
            assert_eq!(
                native.get("suppressConsoleLog"),
                row["options"]["suppressConsoleLog"].as_bool().as_ref(),
                "order-log/native-suppressConsoleLog/{index}"
            );
        }
        compared_corpus += 1;
    }
    assert_eq!(compared_corpus, 175, "ORDERLY-COMPARISONS errors/corpus");
    assert_eq!(
        corpus["boundary"]["firstUnit"], 0xd800,
        "source surrogate captured before serialization"
    );
    assert_eq!(
        corpus["boundary"]["expected"], true,
        "source surrogate classification"
    );
    assert_eq!(
        corpus["boundary"]["options"]["suppressConsoleLog"], true,
        "source surrogate logging hint"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "chosen UTF-8 boundary refusal"
    );
}
