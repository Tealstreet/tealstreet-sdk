use safe_cex_webworker::worker_logging::{WorkerLogLevel, forward_log};
use serde_json::{Value, json};
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
        row["message"].as_str().unwrap(),
        row.get("extra").and_then(Value::as_object),
        |payload| {
            sent.push(serde_json::to_value(payload).unwrap());
            if row["fail"] == true {
                Err(json!({"name":"TypeError","message":"post failed"}))
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
    json!({"sent":sent,"error":result.err(),"keys":keys})
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
            run(row),
            json!({"sent":row["sent"],"error":row["error"],"keys":row["keys"]}),
            "WORKER-LOG {index}"
        );
    }
    let row = &source["inherited"];
    assert_eq!(
        run(row),
        json!({"sent":row["sent"],"error":row["error"],"keys":row["keys"]}),
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
    let ours = run(omitted);
    assert_eq!(
        ours["sent"][0]["data"]["context"]["account"], "a",
        "CHOSEN JSON input loses own undefined overriding the default"
    );
    assert!(ours["sent"][0]["data"]["context"].get("detail").is_none());
    assert!(
        serde_json::from_str::<Value>(source["lone"].as_str().unwrap()).is_err(),
        "shared UTF-8 refusal"
    );
    let native = source["nativeNonfinite"].as_array().unwrap();
    assert_eq!(
        native.iter().map(|r| r["name"].clone()).collect::<Vec<_>>(),
        vec![json!("NaN"), json!("Infinity"), json!("-Infinity")]
    );
    for row in native {
        assert_eq!(row["sourceType"], "number");
        assert_eq!(
            row["sourceKind"], row["name"],
            "native source number survives structured clone"
        );
        let number = match row["name"].as_str().unwrap() {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => unreachable!(),
        };
        let mut input = row["row"].clone();
        input["extra"] = json!({"value":number});
        assert!(
            input["extra"]["value"].is_null(),
            "CHOSEN native JSON input boundary"
        );
        assert!(
            run(&input)["sent"][0]["data"]["context"]["value"].is_null(),
            "CHOSEN native nonfinite is already lost before forwarding"
        );
    }
    let nested = &source["nested"];
    let mut nested_row = nested["row"].clone();
    nested_row["extra"] = serde_json::from_str(nested["inputJson"].as_str().unwrap()).unwrap();
    assert_eq!(
        serde_json::to_string(&run(&nested_row)["sent"][0]).unwrap(),
        nested["wire"].as_str().unwrap(),
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
            "m",
            extra.as_object(),
            |payload| {
                value = payload.data;
                Ok::<_, ()>(())
            },
        )
        .unwrap();
        let value = value.unwrap();
        let numeric = &value["context"]["value"];
        assert!(numeric.is_number(), "WORKER-LOG retained numeric lexeme");
        let number = numeric
            .as_f64()
            .unwrap_or_else(|| safe_cex_num::number_from_string(&numeric.to_string()));
        assert_eq!(
            format!("{:016x}", number.to_bits()),
            row["bits"].as_str().unwrap(),
            "WORKER-LOG numeric bits {}",
            row["token"]
        );
    }
}
