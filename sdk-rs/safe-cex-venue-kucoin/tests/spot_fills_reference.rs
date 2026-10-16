//! Held request-boundary witnesses, not a completed fetchHistoricalFills port.
#[path = "support/numeric_bits.rs"]
mod numeric_bits;
use serde_json::{Value, json};
use std::{
    io::Write,
    process::{Command, Stdio},
};

fn source(cases: &[Value], setting: Option<(&str, &str)>) -> std::process::Output {
    let mut cmd = Command::new("node");
    cmd.arg(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/scripts/spot-fills.cjs"
    ));
    if let Some((key, value)) = setting {
        cmd.env(key, value);
    }
    let mut child = cmd
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    child
        .stdin
        .take()
        .unwrap()
        .write_all(&serde_json::to_vec(cases).unwrap())
        .unwrap();
    child.wait_with_output().unwrap()
}
fn successful(cases: &[Value]) -> Vec<Value> {
    let output = source(cases, None);
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let captures: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        captures.len(),
        cases.len(),
        "KUCOIN-CARDINALITY spot_fills_reference source case count"
    );
    captures
}
fn case() -> Value {
    json!({"opts":{"symbol":"BTC-USDT"},"data":{"code":"200000","data":{"items":[]}}})
}
#[test]
fn live_fill_query_numbers_survive_before_json_projection() {
    let mut cases = vec![];
    for field in ["from", "to", "limit"] {
        for spelling in ["NaN", "Infinity", "-Infinity", "-0", "0", "101", "-1"] {
            let mut c = case();
            c["special"] = json!({field:spelling});
            cases.push(c);
        }
    }
    assert_eq!(
        cases.len(),
        21,
        "KUCOIN-CARDINALITY fills query input total"
    );
    let output = successful(&cases);
    assert_eq!(
        output.len(),
        21,
        "KUCOIN-CARDINALITY fills query source total"
    );
    let mut comparisons = 0;
    for index in 0..21 {
        let c = &cases[index];
        let result = &output[index];
        assert_eq!(
            result["calls"].as_array().unwrap().len(),
            1,
            "KUCOIN-CARDINALITY fills query call count"
        );
        let (field, spelling) = c["special"].as_object().unwrap().iter().next().unwrap();
        let spelling = spelling.as_str().unwrap();
        let key = match field.as_str() {
            "from" => "startAt",
            "to" => "endAt",
            _ => "limit",
        };
        let native = match spelling {
            "NaN" => f64::NAN,
            "Infinity" => f64::INFINITY,
            "-Infinity" => f64::NEG_INFINITY,
            _ => spelling.parse::<f64>().unwrap(),
        };
        let expected = if key == "limit" {
            if native.is_nan() || native == 0.0 {
                100.0
            } else {
                safe_cex_num::math_min(native, 100.0)
            }
        } else {
            native
        };
        let expected_text = safe_cex_num::js_number_string(expected);
        let query = if key == "limit" {
            format!("symbol=BTC-USDT&limit={expected_text}")
        } else {
            format!("symbol=BTC-USDT&limit=100&{key}={expected_text}")
        };
        assert_eq!(
            result["calls"][0]["query"], query,
            "live fills query spelling: {field}/{spelling}"
        );
        assert_eq!(
            result["calls"][0]["bits"][key],
            numeric_bits::bits(expected),
            "live fills query bits: {field}/{spelling}"
        );
        if !expected.is_finite() {
            assert!(
                result["calls"][0]["params"][key].is_null(),
                "JSON projection loses native query number"
            );
        }
        comparisons += 1;
    }
    assert_eq!(
        comparisons, 21,
        "KUCOIN-CARDINALITY fills query compared cases"
    );
    // This is a query-domain limitation, not permission to synthesize NaN JSON.
    assert!(
        safe_cex_types::retained_json::RetainedJsonInput::Float(f64::NAN)
            .into_value()
            .is_err(),
        "shared JSON ingress must refuse NaN"
    );
    for n in [f64::INFINITY, f64::NEG_INFINITY, -0.0] {
        let value = safe_cex_types::retained_json::RetainedJsonInput::Float(n)
            .into_value()
            .unwrap();
        assert_eq!(
            safe_cex_num::numeric_ingress::js_value_as_f64(&value)
                .unwrap()
                .to_bits(),
            n.to_bits()
        );
    }
}
#[test]
fn live_fill_mapper_bits_clock_and_error_order() {
    let mut c = case();
    c["data"]["data"]["items"] = json!([{"symbol":"BTC-USDT","orderId":"fill-order-identity","side":"sell","feeCurrency":"KCS","createdAt":0,"price":"2tail","size":"-0","fee":"1e400"}]);
    let mut later = c.clone();
    later["data"]["data"]["items"]
        .as_array_mut()
        .unwrap()
        .push(Value::Null);
    let output = successful(&[c, later, json!({"opts":{},"data":null})]);
    assert_eq!(
        output.len(),
        3,
        "KUCOIN-CARDINALITY fill mapper source total"
    );
    for field in ["fills", "bits", "keys"] {
        assert_eq!(
            output[0][field].as_array().unwrap().len(),
            1,
            "KUCOIN-CARDINALITY fill mapper {field} rows"
        );
    }
    assert_eq!(
        output[0]["bits"][0],
        json!([
            format!("{:016x}", 1700000000123.0_f64.to_bits()),
            "4000000000000000",
            "8000000000000000",
            "7ff0000000000000"
        ]),
        "live fills native mapper bits"
    );
    assert_eq!(output[0]["clockCalls"], 1);
    for (field, expected) in [
        ("symbol", "BTC-USDT"),
        ("orderId", "fill-order-identity"),
        ("side", "sell"),
        ("commissionAsset", "KCS"),
    ] {
        assert_eq!(
            output[0]["fills"][0][field], expected,
            "live fill string field identity: {field}"
        );
    }
    assert_eq!(
        output[0]["keys"][0],
        json!([
            "timestamp",
            "symbol",
            "orderId",
            "price",
            "qty",
            "side",
            "commission",
            "commissionAsset"
        ])
    );
    assert_eq!(output[1]["fills"], json!([]));
    assert_eq!(
        output[1]["errors"],
        json!(["Cannot read properties of null (reading 'createdAt')"])
    );
    assert_eq!(
        output[1]["clockCalls"], 1,
        "earlier clock call survives later batch failure"
    );
    assert_eq!(output[2]["calls"], json!([]));
    assert_eq!(output[2]["clockCalls"], 0);
}
#[test]
fn live_fill_source_mutation_fails_named_assertion() {
    if std::env::var_os("KUCOIN_FILL_CONTROL_CHILD").is_some() {
        return;
    }
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "live_fill_mapper_bits_clock_and_error_order",
            "--nocapture",
        ])
        .env("KUCOIN_FILL_CONTROL_CHILD", "1")
        .env("KUCOIN_FILLS_MUTATION", "number")
        .output()
        .unwrap();
    assert_eq!(output.status.code(), Some(101));
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("live fills native mapper bits"),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}
#[test]
fn live_fill_dependencies_fail_loudly() {
    for setting in [
        ("KUCOIN_FILLS_MISSING_RUNTIME", "1"),
        (
            "SAFE_CEX_TYPESCRIPT_PATH",
            "missing-kucoin-typescript-control",
        ),
    ] {
        let output = source(&[case()], Some(setting));
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("MODULE_NOT_FOUND"));
    }
}
