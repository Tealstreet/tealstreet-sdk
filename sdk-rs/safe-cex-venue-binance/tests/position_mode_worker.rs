use safe_cex_venue_binance::position_mode::after_binance_method_called_in_worker;
use serde_json::{Value, json};

#[test]
#[ignore = "mandatory live source oracle: yarn venue-oracle:binance"]
fn position_mode_worker_reconciliation_matches_source() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/position-mode-worker.mjs"
        ))
        .output()
        .expect("Node worker position-mode oracle");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 906);
    for (i, case) in cases.iter().enumerate() {
        let args: Vec<Value> = serde_json::from_str(case["rawArgs"].as_str().unwrap()).unwrap();
        let mut trace = vec![];
        let sentinel = String::from("fixture confirmation");
        let result = after_binance_method_called_in_worker(
            case["method"].as_str().unwrap(),
            &args,
            |hedged, symbol| {
                let mut item = json!({"hedged":hedged, "symbolType":if symbol.is_some(){"string"}else{"undefined"}});
                if let Some(symbol) = symbol {
                    item["symbol"] = json!(symbol);
                }
                trace.push(item);
                if case["reject"] == true {
                    Err(&sentinel)
                } else {
                    Ok(())
                }
            },
        );
        let error = match result {
            Ok(()) => Value::Null,
            Err(error) => {
                json!({"same":std::ptr::eq(error, &sentinel), "name":"RangeError", "message":error})
            }
        };
        assert_eq!(
            json!({"trace":trace,"error":error}),
            json!({"trace":case["trace"],"error":case["error"]}),
            "worker position-mode case{i} method={} args={}",
            case["method"],
            case["rawArgs"]
        );
    }
}
