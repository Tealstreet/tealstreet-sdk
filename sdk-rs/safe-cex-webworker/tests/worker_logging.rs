use safe_cex_exchange::js_runtime::value::{Js, from_json, to_json};
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
            json!(is_record(&from_json(&record["input"]))),
            record["isRecord"],
            "record {i}"
        );
        assert_eq!(
            numbers(
                to_json(&get_worker_sentry_control(&from_json(&record["input"])).unwrap()).unwrap()
            ),
            numbers(record["control"].clone()),
            "control {i}"
        );
        assert_eq!(
            numbers(
                to_json(&split_worker_log_extra(Some(&from_json(&record["input"]))).unwrap())
                    .unwrap()
            ),
            numbers(record["split"].clone()),
            "extra {i}"
        );
    }
    assert_eq!(
        to_json(&split_worker_log_extra(None).unwrap()).unwrap(),
        json!({"extra":{},"sentryTags":{},"sentryExtra":{},"suppressConsoleLog":false})
    );
    for witness in data["nonfiniteSuppression"].as_array().unwrap() {
        assert_eq!(witness["input"]["suppressConsoleLog"], Value::Null);
        assert_eq!(witness["split"]["suppressConsoleLog"], true);
        assert_eq!(
            split_worker_log_extra(Some(&Js::object(vec![(
                "suppressConsoleLog",
                Js::Number(if witness["split"]["suppressConsoleLog"] == true {
                    f64::INFINITY
                } else {
                    0.0
                })
            )])))
            .unwrap()
            .get("suppressConsoleLog")
            .unwrap()
            .truthy(),
            true
        );
    }
    for witness in data["overflowJson"].as_array().unwrap() {
        let input: Value = serde_json::from_str(witness["inputJson"].as_str().unwrap()).unwrap();
        assert_eq!(
            to_json(&split_worker_log_extra(Some(&from_json(&input))).unwrap()).unwrap(),
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
    let prototype = from_json(
        &json!({"safeCexSentryReportKey":"inherited","safeCexSentryReportCooldownMs":1,"safeCexSentryTags":{"scope":"prototype"},"safeCexSentryExtra":{"detail":1},"sentryTags":{"scope":"prototype"},"sentryExtra":{"detail":2},"suppressConsoleLog":true}),
    );
    let value = Js::object(vec![]);
    if let Js::Object(object) = &value {
        *object.proto.borrow_mut() = Some(prototype);
    }
    assert_eq!(
        numbers(to_json(&get_worker_sentry_control(&value).unwrap()).unwrap()),
        numbers(inherited["control"].clone())
    );
    assert_eq!(
        numbers(to_json(&split_worker_log_extra(Some(&value)).unwrap()).unwrap()),
        numbers(inherited["split"].clone())
    );
    let hidden = Js::object(vec![]);
    if let Js::Object(object) = &hidden {
        object.props.borrow_mut().insert(
            "safeCexSentryReportKey"
                .encode_utf16()
                .collect::<Vec<_>>()
                .into(),
            safe_cex_exchange::js_runtime::value::Prop::DataDescriptor {
                value: Js::str("hidden"),
                writable: false,
                enumerable: false,
                configurable: false,
            },
        );
    }
    assert_eq!(
        to_json(&get_worker_sentry_control(&hidden).unwrap()).unwrap(),
        data["omissions"][1]["control"]
    );
    let reads = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let controlled = Js::object(vec![]);
    for (key, value) in [
        ("safeCexSentryReportKey", Js::str("key")),
        ("safeCexSentryReportCooldownMs", Js::Number(5.0)),
        ("safeCexSentryTags", Js::object(vec![("t", Js::Bool(true))])),
        (
            "safeCexSentryExtra",
            Js::object(vec![("e", Js::Bool(true))]),
        ),
    ] {
        let reads = reads.clone();
        controlled
            .define_accessor(
                key,
                Some(std::rc::Rc::new(move |_| {
                    reads.borrow_mut().push(key);
                    Ok(value.clone())
                })),
                None,
            )
            .unwrap();
    }
    get_worker_sentry_control(&controlled).unwrap();
    assert_eq!(
        json!(*reads.borrow()),
        data["controlReads"],
        "WORKER-LOG actual control getter order/count"
    );
    reads.borrow_mut().clear();
    let split = Js::object(vec![]);
    for (key, value) in [
        ("sentryTags", Js::object(vec![("t", Js::Bool(true))])),
        ("sentryExtra", Js::object(vec![("e", Js::Bool(true))])),
        ("suppressConsoleLog", Js::Number(f64::INFINITY)),
        ("plain", Js::Number(3.0)),
    ] {
        let reads = reads.clone();
        split
            .define_accessor(
                key,
                Some(std::rc::Rc::new(move |_| {
                    reads.borrow_mut().push(key);
                    Ok(value.clone())
                })),
                None,
            )
            .unwrap();
    }
    split_worker_log_extra(Some(&split)).unwrap();
    assert_eq!(
        json!(*reads.borrow()),
        data["splitReads"],
        "WORKER-LOG Object.entries gets reserved fields before filtering"
    );
}
