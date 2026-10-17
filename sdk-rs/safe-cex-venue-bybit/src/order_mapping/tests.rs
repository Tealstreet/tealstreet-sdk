use super::*;
use serde_json::json;
fn norm(v: Value) -> Value {
    match v {
        Value::Number(n) => {
            let n = n.as_f64().unwrap();
            json!(if n == 0. { 0. } else { n })
        }
        Value::Array(a) => Value::Array(a.into_iter().map(norm).collect()),
        Value::Object(o) => Value::Object(o.into_iter().map(|(k, v)| (k, norm(v))).collect()),
        v => v,
    }
}
fn class(n: f64) -> &'static str {
    if n.is_nan() {
        "NaN"
    } else if n == f64::INFINITY {
        "Infinity"
    } else if n == f64::NEG_INFINITY {
        "-Infinity"
    } else if n == 0. && n.is_sign_negative() {
        "-0"
    } else {
        "finite"
    }
}
#[test]
#[ignore = "requires workspace Node; mandatory venue-oracle:bybit"]
fn spot_and_inverse_orders_match_live_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/order-mapping-fixtures.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    for (i, c) in cases.iter().enumerate() {
        let mut calls = 0;
        let result = map_order(
            &c["row"],
            c["variant"] == "bybitinverse",
            c["marketInverse"] == true,
            || {
                let n = 123456789. + calls as f64;
                calls += 1;
                n
            },
        );
        assert_eq!(json!(calls), c["clockCalls"], "clock case{i}");
        if let Some(error) = c.get("error") {
            assert_eq!(
                result.unwrap_err(),
                error.as_str().unwrap(),
                "error case{i}"
            );
            continue;
        }
        let result = result.unwrap_or_else(|e| panic!("case{i}: {e}; {c}"));
        assert_eq!(
            norm(serde_json::to_value(&result).unwrap()),
            norm(c["result"].clone()),
            "orders case{i}: {c}"
        );
        let classes: Vec<_> = result
            .iter()
            .map(|o| {
                [
                    o.price.to_number(),
                    o.amount,
                    o.filled,
                    o.remaining,
                    o.extra.as_ref().unwrap().remaining_native.unwrap(),
                    o.extra.as_ref().unwrap().remaining_notional.unwrap_or(0.),
                ]
                .map(class)
            })
            .collect();
        assert_eq!(json!(classes), c["classes"], "numeric classes case{i}");
    }
    assert_eq!(cases.len(), 940);
}
