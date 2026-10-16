use safe_cex_venue_polymarket::constants::*;
use serde::Serialize;
use serde_json::{Map, Value, json};
use std::process::Command;

fn entries<K: Serialize, V: Serialize>(items: &[(K, V)]) -> Value {
    Value::Object(
        items
            .iter()
            .map(|(key, value)| {
                (
                    serde_json::to_value(key)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(value).unwrap(),
                )
            })
            .collect(),
    )
}

fn source_leaf_count(value: &Value) -> usize {
    match value {
        Value::Object(values) => values.values().map(source_leaf_count).sum(),
        Value::Array(values) => values.iter().map(source_leaf_count).sum(),
        _ => 1,
    }
}

fn compare(actual: &Value, expected: &Value, label: &str) -> usize {
    match (actual, expected) {
        (Value::Object(actual), Value::Object(expected)) => {
            assert_eq!(
                actual.keys().collect::<Vec<_>>(),
                expected.keys().collect::<Vec<_>>(),
                "{label}:own_keys"
            );
            actual
                .iter()
                .map(|(key, value)| compare(value, &expected[key], label))
                .sum()
        }
        (Value::Array(actual), Value::Array(expected)) => {
            assert_eq!(actual.len(), expected.len(), "{label}:array_length");
            actual
                .iter()
                .zip(expected)
                .map(|(a, b)| compare(a, b, label))
                .sum()
        }
        (Value::Number(actual), Value::Number(expected)) => {
            assert_eq!(
                actual.as_f64().unwrap().to_bits(),
                expected.as_f64().unwrap().to_bits(),
                "{label}:number"
            );
            1
        }
        _ => {
            assert_eq!(actual, expected, "{label}:value");
            1
        }
    }
}

#[test]
fn constants_actual_typescript() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/constants_oracle.cjs"
        ))
        .output()
        .expect("Node is required");
    assert!(
        output.status.success(),
        "constants_oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let variants = [
        ("EOA", SignatureType::Eoa),
        ("POLY_PROXY", SignatureType::PolyProxy),
        ("POLY_GNOSIS_SAFE", SignatureType::PolyGnosisSafe),
        ("POLY_1271", SignatureType::Poly1271),
    ];
    // JS numeric enum keys enumerate before its named keys.
    let mut signature = Map::new();
    for (name, variant) in variants {
        signature.insert((variant as u8).to_string(), json!(name));
    }
    for (name, variant) in variants {
        signature.insert(name.into(), json!(variant as u8));
    }
    let rows = [
        (
            "POLYMARKET_CONFIG",
            serde_json::to_value(POLYMARKET_CONFIG).unwrap(),
        ),
        ("ENDPOINTS", entries(ENDPOINTS)),
        ("ORDER_TYPE_MAP", entries(ORDER_TYPE_MAP)),
        ("ORDER_SIDE_MAP", entries(ORDER_SIDE_MAP)),
        ("ORDER_SIDE_REVERSE", entries(ORDER_SIDE_REVERSE)),
        ("ORDER_STATUS_MAP", entries(ORDER_STATUS_MAP)),
        ("SignatureType", Value::Object(signature)),
        (
            "EIP712_DOMAIN",
            serde_json::to_value(EIP712_DOMAIN).unwrap(),
        ),
        ("EIP712_ORDER_TYPES", entries(EIP712_ORDER_TYPES)),
        ("WS_MESSAGE_TYPES", entries(WS_MESSAGE_TYPES)),
        ("WS_CONFIG", serde_json::to_value(WS_CONFIG).unwrap()),
        ("RATE_LIMITS", serde_json::to_value(RATE_LIMITS).unwrap()),
        ("TIMEFRAME_MAP", entries(TIMEFRAME_MAP)),
    ];
    assert_eq!(
        oracle["constants"].as_object().unwrap().len(),
        rows.len(),
        "constants_export_census"
    );
    let expected_leaves = [15, 28, 4, 2, 2, 12, 8, 3, 22, 9, 4, 3, 6];
    assert_eq!(rows.len(), expected_leaves.len(), "constant_case_census");
    let mut comparisons = 0;
    for (index, (name, actual)) in rows.into_iter().enumerate() {
        assert_eq!(
            source_leaf_count(&oracle["constants"][name]),
            expected_leaves[index],
            "constant_source_case_leaves:{name}"
        );
        comparisons += compare(
            &actual,
            &oracle["constants"][name],
            &format!("constant:{name}"),
        );
    }

    assert_eq!(comparisons, 118, "constant_total_native_leaf_comparisons");

    // Observe native numbers before serde can normalize a wire value.
    let mut native_numbers = vec![
        (
            "/POLYMARKET_CONFIG/CHAIN_ID".to_owned(),
            POLYMARKET_CONFIG.chain_id,
        ),
        (
            "/POLYMARKET_CONFIG/FEES/TAKER_FEE_BPS".to_owned(),
            POLYMARKET_CONFIG.fees.taker_fee_bps,
        ),
        (
            "/POLYMARKET_CONFIG/FEES/MAKER_FEE_BPS".to_owned(),
            POLYMARKET_CONFIG.fees.maker_fee_bps,
        ),
    ];
    for (name, variant) in variants {
        native_numbers.push((format!("/SignatureType/{name}"), f64::from(variant as u8)));
    }
    native_numbers.extend([
        ("/EIP712_DOMAIN/chainId".to_owned(), EIP712_DOMAIN.chain_id),
        (
            "/WS_CONFIG/PING_INTERVAL".to_owned(),
            WS_CONFIG.ping_interval,
        ),
        ("/WS_CONFIG/PONG_TIMEOUT".to_owned(), WS_CONFIG.pong_timeout),
        (
            "/WS_CONFIG/RECONNECT_DELAY".to_owned(),
            WS_CONFIG.reconnect_delay,
        ),
        (
            "/WS_CONFIG/MAX_RECONNECT_ATTEMPTS".to_owned(),
            WS_CONFIG.max_reconnect_attempts,
        ),
        ("/RATE_LIMITS/PUBLIC_RPS".to_owned(), RATE_LIMITS.public_rps),
        (
            "/RATE_LIMITS/PRIVATE_RPS".to_owned(),
            RATE_LIMITS.private_rps,
        ),
        (
            "/RATE_LIMITS/ORDERS_PER_SECOND".to_owned(),
            RATE_LIMITS.orders_per_second,
        ),
    ]);
    let bits: Value = native_numbers
        .iter()
        .map(|(path, value)| json!([path, "number", format!("{:016x}", value.to_bits())]))
        .collect();
    // Export initialization order can differ; each numeric field is identified by its full path.
    let expected = oracle["numbers"].as_array().unwrap();
    assert_eq!(expected.len(), 15, "constant_source_numeric_count");
    assert_eq!(
        native_numbers.len(),
        expected.len(),
        "native_constant_number_census"
    );
    for item in bits.as_array().unwrap() {
        assert!(
            expected.contains(item),
            "native_constant_bits_before_json:{item}"
        );
    }
}
