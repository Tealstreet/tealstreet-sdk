use safe_cex_exchange::{
    js_runtime::value::{Js, from_json, to_json},
    runtime::ExchangeError,
};
use safe_cex_webworker::worker_logging::{WorkerLogLevel, forward_log};
use serde_json::{Value, json};
fn numbers(value: Value) -> Value {
    match value {
        Value::Number(n) => json!(n.as_f64().unwrap()),
        Value::Array(a) => Value::Array(a.into_iter().map(numbers).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, numbers(v))).collect()),
        other => other,
    }
}
fn run(row: &Value) -> Value {
    let mut sent = Vec::new();
    let result = forward_log(
        serde_json::from_value(row["exchange"].clone()).unwrap(),
        row["account"].as_str().unwrap(),
        match row["level"].as_str().unwrap() {
            "info" => WorkerLogLevel::Info,
            "warn" => WorkerLogLevel::Warn,
            "error" => WorkerLogLevel::Error,
            other => panic!("unexpected source log level {other}"),
        },
        &from_json(&row["message"]),
        row.get("extra").map(from_json).as_ref(),
        |payload| {
            sent.push(json!({"op":"log","data":to_json(&payload.data).unwrap()}));
            if row["fail"] == true {
                Err({
                    let mut error = ExchangeError::new("post failed");
                    error.name = "TypeError".into();
                    error
                })
            } else {
                Ok(())
            }
        },
    );
    let keys: Vec<_> = sent[0]["data"]["context"]
        .as_object()
        .unwrap()
        .keys()
        .cloned()
        .collect();
    json!({"sent":sent,"error":result.err().map(|e|json!({"name":e.name,"message":e.message})),"keys":keys})
}
#[test]
fn forwarding_matches_live_worker_log_and_propagates_post_failure() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/worker_log_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        serde_json::to_value([
            WorkerLogLevel::Info,
            WorkerLogLevel::Warn,
            WorkerLogLevel::Error
        ])
        .unwrap(),
        source["levels"],
        "WORKER-LOG inline literal union"
    );
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 810);
    for (index, row) in rows.iter().enumerate() {
        assert_eq!(
            numbers(run(row)),
            numbers(json!({"sent":row["sent"],"error":row["error"],"keys":row["keys"]})),
            "WORKER-LOG {index}"
        );
    }
    let row = &source["inherited"];
    assert_eq!(
        numbers(run(row)),
        numbers(json!({"sent":row["sent"],"error":row["error"],"keys":row["keys"]})),
        "spread ignores inherited properties"
    );
    let omitted = &source["ownUndefined"];
    assert_eq!(omitted["ownAccount"], true);
    assert_eq!(omitted["ownDetail"], true);
    assert!(
        omitted["sent"][0]["data"]["context"]
            .get("account")
            .is_none()
    );
    let retained = Js::object(vec![("account", Js::Undefined), ("detail", Js::Undefined)]);
    let sent = std::cell::RefCell::new(None);
    forward_log(
        safe_cex_types::ExchangeName::Bybit,
        "a",
        WorkerLogLevel::Info,
        &Js::str("m"),
        Some(&retained),
        |p| {
            *sent.borrow_mut() = Some(p.data);
            Ok::<_, ExchangeError>(())
        },
    )
    .unwrap();
    let context = sent.borrow().as_ref().unwrap().get("context").unwrap();
    for key in ["account", "detail"] {
        assert!(context.has_own(key));
        assert!(context.get(key).unwrap().is_undefined());
    }
    assert_eq!(source["nativeNonfinite"].as_array().unwrap().len(), 3);
    for row in source["nativeNonfinite"].as_array().unwrap() {
        let number = match row["name"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => unreachable!(),
        };
        let context = Js::object(vec![("value", Js::Number(number))]);
        let sent = std::cell::RefCell::new(None);
        forward_log(
            safe_cex_types::ExchangeName::Bybit,
            "a",
            WorkerLogLevel::Info,
            &Js::str("m"),
            Some(&context),
            |p| {
                *sent.borrow_mut() = Some(p.data);
                Ok::<_, ExchangeError>(())
            },
        )
        .unwrap();
        assert_eq!(
            sent.borrow()
                .as_ref()
                .unwrap()
                .get("context")
                .unwrap()
                .get("value")
                .unwrap()
                .to_number()
                .unwrap()
                .to_bits(),
            number.to_bits()
        );
    }
    let lone = Js::utf16(&[0xd800]);
    let sent = std::cell::RefCell::new(None);
    forward_log(
        safe_cex_types::ExchangeName::Bybit,
        "a",
        WorkerLogLevel::Info,
        &lone,
        None,
        |p| {
            *sent.borrow_mut() = Some(p.data);
            Ok::<_, ExchangeError>(())
        },
    )
    .unwrap();
    assert_eq!(
        sent.borrow()
            .as_ref()
            .unwrap()
            .get("message")
            .unwrap()
            .to_js_string()
            .unwrap()
            .as_ref(),
        &[0xd800]
    );
    let nested = &source["nested"];
    let mut nested_row = nested["row"].clone();
    nested_row["extra"] = serde_json::from_str(nested["inputJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_string(&numbers(run(&nested_row)["sent"][0].clone())).unwrap(),
        serde_json::to_string(&numbers(
            serde_json::from_str(nested["wire"].as_str().unwrap()).unwrap()
        ))
        .unwrap(),
        "WORKER-LOG nested enumeration"
    );
    let number_rows = source["numbers"].as_array().unwrap();
    assert_eq!(number_rows.len(), 5);
    for row in number_rows {
        let extra: Value =
            serde_json::from_str(&format!("{{\"value\":{}}}", row["token"].as_str().unwrap()))
                .unwrap();
        let mut value = None;
        forward_log(
            safe_cex_types::ExchangeName::Bybit,
            "a",
            WorkerLogLevel::Info,
            &Js::str("m"),
            Some(&from_json(&extra)),
            |payload| {
                value = Some(payload.data);
                Ok::<_, ExchangeError>(())
            },
        )
        .unwrap();
        let value = value.unwrap();
        let numeric = value.get("context").unwrap().get("value").unwrap();
        let number = numeric.to_number().unwrap();
        assert_eq!(
            format!("{:016x}", number.to_bits()),
            row["bits"].as_str().unwrap(),
            "WORKER-LOG numeric bits {}",
            row["token"]
        );
    }
}
