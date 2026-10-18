use safe_cex_venue_mexc::utils::*;
use serde_json::Value;

#[test]
fn mapper_symbols_keep_native_scalar_units_and_name_whole_record_refusals() {
    const NAMES: [&str; 7] = [
        "normalizeMarket",
        "normalizeTicker",
        "normalizePosition",
        "normalizeOrder",
        "normalizePlanOrder",
        "normalizeStopOrder",
        "normalizeExecution",
    ];
    let symbols: [(&str, &str, &[u16]); 5] = [
        ("ascii", "BTC_USDT", &[66, 84, 67, 85, 83, 68, 84]),
        ("bmp", "Ω_USDT", &[937, 85, 83, 68, 84]),
        ("astral", "🚀_USDT", &[55357, 56960, 85, 83, 68, 84]),
        (
            "internal-space",
            "A\u{feff}\u{3000}_USDT",
            &[65, 65279, 12288, 85, 83, 68, 84],
        ),
        ("lone", "", &[55296, 85, 83, 68, 84]),
    ];
    let mut subject_calls = 0;
    let mut market_children = 0;
    let mut refused = 0;
    let mut observations = Vec::new();
    for name in NAMES {
        for (label, symbol, expected_units) in symbols {
            let symbol_json = if label == "lone" {
                r#""\ud800_USDT""#.to_owned()
            } else {
                serde_json::to_string(symbol).unwrap()
            };
            let input = format!(
                r#"{{"symbol":{symbol_json},"id":"42","orderId":"43","positionType":1,"side":1,"state":1,"vol":5,"holdVol":7,"price":123,"holdAvgPrice":11,"createTime":1700000000000,"contractSize":1,"leverage":3}}"#
            );
            let parsed = serde_json::from_str::<Value>(&input);
            if label == "lone" {
                let error = parsed.expect_err(
                    "unicode native whole Value ingress refuses lone surrogate before helper",
                );
                assert!(
                    error.is_syntax(),
                    "unicode native refusal is JSON string syntax {name}"
                );
                refused += 1;
                continue;
            }
            let raw = parsed.unwrap();
            let raw = raw.as_object().unwrap();
            let (native_symbol, discriminator) = match name {
                "normalizeMarket" => {
                    let v =
                        normalize_market(&serde_json::Value::Object((raw).clone()).into()).unwrap();
                    (v.symbol, v.contract_size.unwrap())
                }
                "normalizeTicker" => {
                    let market =
                        normalize_market(&serde_json::Value::Object((raw).clone()).into()).unwrap();
                    market_children += 1;
                    let v = normalize_ticker(raw, &market, &|ms| ms).unwrap();
                    (v.symbol, v.volume)
                }
                "normalizePosition" => {
                    let v = normalize_position(raw, None, None, &|ms| ms)
                        .unwrap()
                        .unwrap();
                    (v.symbol, v.contracts)
                }
                "normalizeOrder" => {
                    let v = normalize_order(raw, None, &|ms| ms).unwrap().unwrap();
                    (v.symbol, v.amount)
                }
                "normalizePlanOrder" => {
                    let v = normalize_plan_order(raw, None, &|ms| ms).unwrap().unwrap();
                    (v.symbol, v.amount)
                }
                "normalizeStopOrder" => {
                    let v = normalize_stop_order(raw, None, &|ms| ms).unwrap().unwrap();
                    (v.symbol, v.amount)
                }
                "normalizeExecution" => {
                    let v = normalize_execution(raw, None, &|ms| ms).unwrap().unwrap();
                    (v.symbol, v.qty)
                }
                _ => unreachable!(),
            };
            subject_calls += 1;
            assert_eq!(
                native_symbol.encode_utf16().collect::<Vec<_>>(),
                expected_units,
                "unicode native materialized symbol {name}/{label}"
            );
            let expected: f64 = match name {
                "normalizeMarket" => 1.0,
                "normalizeTicker" => 0.0,
                "normalizePosition" => 7.0,
                _ => 5.0,
            };
            assert_eq!(
                discriminator.to_bits(),
                expected.to_bits(),
                "unicode native independent numeric discriminator {name}/{label}"
            );
            observations.push((name, label, expected_units));
        }
    }
    assert_eq!(
        subject_calls, 28,
        "unicode native independent subject call floor"
    );
    assert_eq!(
        market_children, 4,
        "unicode native independent ticker market child floor"
    );
    assert_eq!(
        refused, 7,
        "unicode native independent whole ingress refusal floor"
    );
    assert_eq!(
        observations.len(),
        28,
        "unicode native independent admitted result floor"
    );
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/mapper_unicode_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "unicode actual maintained source oracle {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        source["subjectCalls"], 35,
        "unicode source independent subject call floor"
    );
    assert_eq!(
        source["marketChildren"], 5,
        "unicode source independent ticker market child floor"
    );
    let rows = source["records"].as_array().unwrap();
    assert_eq!(rows.len(), 35, "unicode source independent result floor");
    for name in NAMES {
        for (label, _, expected_units) in symbols {
            let row = rows
                .iter()
                .find(|r| r["name"] == name && r["label"] == label)
                .expect("unicode actual source name/label covered");
            assert_eq!(
                row["symbolUnits"],
                serde_json::json!(expected_units),
                "unicode source materialized symbol {name}/{label}"
            );
        }
    }
}
