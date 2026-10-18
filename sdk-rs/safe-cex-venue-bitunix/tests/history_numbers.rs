use safe_cex_num::{JsCoercionError, numeric_ingress::js_deserialize_passthrough};
use safe_cex_venue_bitunix::history_numbers::{
    income_to_number, order_to_number, order_to_optional_number,
};
use serde_json::{Value, json};
use std::process::Command;

fn observe(value: Result<Option<f64>, JsCoercionError>) -> Value {
    match value {
        Ok(Some(n)) => {
            json!({"type":"number","bits":format!("{:016x}", if n.is_nan() { 0x7ff8_0000_0000_0000 } else { n.to_bits() })})
        }
        Ok(None) => json!({"type":"undefined"}),
        Err(error) => json!({"error":error.name,"message":error.message}),
    }
}

#[test]
fn actual_typescript_history_numbers() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/history-numbers-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "history actual source execution: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 51, "HISTORY source total");
    let mut compared = [0; 3];
    let mut refused = 0;
    for row in rows {
        let raw = row["raw"].as_str();
        assert_eq!(
            row["results"].as_object().unwrap().len(),
            3,
            "HISTORY per-case helper count"
        );
        let value = match raw {
            None => None,
            Some(raw) => {
                match js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)) {
                    Ok(v) => Some(v),
                    Err(_) => {
                        assert!(
                            ["\"\\ud800\"", "\"12.5\\ud800\""].contains(&raw),
                            "HISTORY only declared UTF8 refusal"
                        );
                        let expected = if raw == "\"\\ud800\"" {
                            [
                                json!({"type":"undefined"}),
                                json!({"type":"number","bits":"0000000000000000"}),
                                json!({"type":"number","bits":"0000000000000000"}),
                            ]
                        } else {
                            [
                                json!({"type":"number","bits":"4029000000000000"}),
                                json!({"type":"number","bits":"4029000000000000"}),
                                json!({"type":"number","bits":"4029000000000000"}),
                            ]
                        };
                        for (i, name) in [
                            "bitunix.order-history.ts:toOptionalNumber",
                            "bitunix.order-history.ts:toNumber",
                            "bitunix.income.ts:toNumber",
                        ]
                        .iter()
                        .enumerate()
                        {
                            assert_eq!(
                                row["results"][name], expected[i],
                                "HISTORY source surrogate {name}"
                            );
                        }
                        refused += 1;
                        continue;
                    }
                }
            }
        };
        compared[0] += 1;
        assert_eq!(
            observe(order_to_optional_number(value.as_ref())),
            row["results"]["bitunix.order-history.ts:toOptionalNumber"],
            "HISTORY native order optional {raw:?}"
        );
        compared[1] += 1;
        assert_eq!(
            observe(order_to_number(value.as_ref()).map(Some)),
            row["results"]["bitunix.order-history.ts:toNumber"],
            "HISTORY native order number {raw:?}"
        );
        compared[2] += 1;
        assert_eq!(
            observe(income_to_number(value.as_ref()).map(Some)),
            row["results"]["bitunix.income.ts:toNumber"],
            "HISTORY native income number {raw:?}"
        );
    }
    assert_eq!(
        compared,
        [49, 49, 49],
        "HISTORY independent native comparison counts"
    );
    assert_eq!(refused, 2, "HISTORY UTF8 refusal count");
}
