use safe_cex_webworker::worker_logging::*;
use serde_json::{Value, json};
fn numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        other => other,
    }
}
#[test]
fn sentry_controls_and_log_extras_match_live_worker_helpers() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_logging_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let data: Value = serde_json::from_slice(&output.stdout).unwrap();
    let records = data["records"].as_array().unwrap();
    assert!(records.len() > 500);
    for (i, record) in records.iter().enumerate() {
        assert_eq!(
            json!(is_record(&record["input"])),
            record["isRecord"],
            "record {i}"
        );
        assert_eq!(
            numbers(get_worker_sentry_control(&record["input"])),
            numbers(record["control"].clone()),
            "control {i}"
        );
        assert_eq!(
            numbers(split_worker_log_extra(Some(&record["input"]))),
            numbers(record["split"].clone()),
            "extra {i}"
        );
    }
    assert_eq!(
        split_worker_log_extra(None),
        json!({"extra":{},"sentryTags":{},"sentryExtra":{},"suppressConsoleLog":false})
    );
    for witness in data["nonfiniteSuppression"].as_array().unwrap() {
        assert_eq!(witness["input"]["suppressConsoleLog"], Value::Null);
        assert_eq!(witness["split"]["suppressConsoleLog"], true);
        assert_eq!(
            split_worker_log_extra(Some(&witness["input"]))["suppressConsoleLog"],
            false
        );
    }
    for witness in data["overflowJson"].as_array().unwrap() {
        let input: Value = serde_json::from_str(witness["inputJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            split_worker_log_extra(Some(&input)),
            witness["split"],
            "live overflowing JSON truthiness: {}",
            witness["inputJson"]
        );
    }
    let inherited = &data["omissions"][0];
    assert_eq!(inherited["input"], json!({}));
    assert_eq!(inherited["control"]["reportKey"], "inherited");
    assert_eq!(inherited["control"]["tags"], json!({"scope":"prototype"}));
    assert_eq!(inherited["split"]["suppressConsoleLog"], true);
    assert_eq!(get_worker_sentry_control(&inherited["input"]), json!({}));
    assert_eq!(
        split_worker_log_extra(Some(&inherited["input"]))["suppressConsoleLog"],
        false
    );
    assert_eq!(data["omissions"][1]["control"]["reportKey"], "hidden");
    assert_eq!(
        get_worker_sentry_control(&data["omissions"][1]["input"]),
        json!({})
    );
}
