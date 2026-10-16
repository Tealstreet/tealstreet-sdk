use safe_cex_webworker::task_types::{
    CHASER_KEPT_ORDER_STOP_REASON, CHASER_UNCONFIRMED_ORDER_STOP_REASON,
};
use serde_json::Value;

#[test]
fn chaser_stop_reasons_match_executed_source_exports() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/chaser_stop_reasons_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let capture: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(!capture["architecture"].as_str().unwrap().is_empty());
    assert!(capture["nodeMajor"].as_u64().unwrap() > 0);
    let values = capture["values"].as_object().unwrap();
    assert_eq!(values.len(), 2, "CHASER-STOP source export count");
    let mut comparisons = 0;
    for (name, native) in [
        (
            "CHASER_KEPT_ORDER_STOP_REASON",
            CHASER_KEPT_ORDER_STOP_REASON,
        ),
        (
            "CHASER_UNCONFIRMED_ORDER_STOP_REASON",
            CHASER_UNCONFIRMED_ORDER_STOP_REASON,
        ),
    ] {
        let source = values.get(name).expect("CHASER-STOP required export");
        assert_eq!(
            source.as_str().unwrap(),
            native,
            "CHASER-STOP native identity {name}"
        );
        comparisons += 1;
    }
    assert_eq!(comparisons, 2, "CHASER-STOP native comparison count");
}
