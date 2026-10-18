mod support;
use safe_cex_types::PositionMode;
use safe_cex_venue_mexc::utils::*;
use serde_json::{Value, json};
use support::{decode_number, encode};
fn undefined(v: &Value) -> bool {
    v.get("$undefined") == Some(&Value::Bool(true))
}
fn decoded(v: &Value) -> Value {
    if v.get("$number").is_some() {
        return json!(decode_number(v));
    }
    match v {
        Value::Array(a) => Value::Array(a.iter().map(decoded).collect()),
        Value::Object(o) => Value::Object(
            o.iter()
                .filter(|(_, v)| !undefined(v))
                .map(|(k, v)| (k.clone(), decoded(v)))
                .collect(),
        ),
        _ => v.clone(),
    }
}

#[test]
fn mexc_mappers_live_typescript_every_value() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/mappers_oracle.mjs"
        ))
        .env("TZ", "UTC")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 279);
    let mut findings = 0;
    for (index, case) in cases.iter().enumerate() {
        let raw = decoded(&case["raw"]);
        let raw = raw.as_object().unwrap();
        let market = case.get("marketRaw").map(|v| {
            normalize_market(&serde_json::Value::Object((v.as_object().unwrap()).clone()).into())
                .unwrap()
        });
        let mode = if case["mode"] == "oneway" {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        };
        let actual = match case["name"].as_str().unwrap() {
            "normalizePosition" => {
                normalize_position(raw, market.as_ref(), Some(mode), &|ms| ms).map(|v| encode(&v))
            }
            "normalizeOrder" => normalize_order(raw, market.as_ref(), &|ms| ms).map(|v| encode(&v)),
            "normalizePlanOrder" => {
                normalize_plan_order(raw, market.as_ref(), &|ms| ms).map(|v| encode(&v))
            }
            "normalizeStopOrder" => {
                normalize_stop_order(raw, market.as_ref(), &|ms| ms).map(|v| encode(&v))
            }
            "normalizeExecution" => {
                normalize_execution(raw, market.as_ref(), &|ms| ms).map(|v| encode(&v))
            }
            name => panic!("unknown {name}"),
        };
        // Explicit acceptance witnesses: TS carries a value the shared field
        // cannot represent. These must flip when the shared contract changes.
        if case["name"] == "normalizeExecution" && raw.contains_key("feeAsset") {
            let error = actual.unwrap_err();
            assert_eq!(
                error.message,
                "MEXC commissionAsset is outside the shared Execution string boundary"
            );
            assert!(case["output"].get("commissionAsset").is_some());
            findings += 1;
            continue;
        }
        match actual {
            Ok(value) => assert_eq!(
                value, case["output"],
                "mapper case {index}: {} {}",
                case["name"], case["raw"]
            ),
            Err(error) => assert_eq!(
                json!({"name":error.name,"message":error.message}),
                case["error"],
                "mapper case {index}: {} {}",
                case["name"],
                case["raw"]
            ),
        }
    }
    assert_eq!(findings, 5);
}

#[test]
fn mexc_mapper_typescript_negative_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_mappers_live_typescript_every_value",
            "--nocapture",
        ])
        .env("MEXC_TS_CONTROL", "plan-state")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_mappers_live_typescript_every_value ... FAILED"),
        "{text}"
    );
    assert!(
        text.contains("normalizePlanOrder") && text.contains("assertion `left == right` failed"),
        "{text}"
    );
}
