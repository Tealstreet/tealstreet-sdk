use num_bigint::BigInt;
use safe_cex_venue_lighter::transaction_client_helpers::*;
use serde_json::Value;
use std::process::Command;
fn corpus() -> Value {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/transaction-client-helpers-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn units(v: &Value) -> Vec<u16> {
    v.as_array()
        .unwrap()
        .iter()
        .map(|n| u16::try_from(n.as_u64().unwrap()).unwrap())
        .collect()
}
fn optional(v: &Value) -> Option<Vec<u16>> {
    v.as_str().map(|s| s.encode_utf16().collect())
}
fn number(value: &str) -> f64 {
    match value {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => value.parse().unwrap(),
    }
}
fn result(native: Result<String, Vec<u16>>, expected: &Value, label: &str) {
    match native {
        Ok(value) => assert_eq!(
            Some(value.as_str()),
            expected["ok"].as_str(),
            "{label} native success"
        ),
        Err(value) => assert_eq!(
            value,
            units(&expected["error"]),
            "{label} native UTF16 error"
        ),
    }
}
#[test]
#[ignore = "actual source Node oracle"]
fn transaction_client_native_helpers() {
    let corpus = corpus();
    assert_eq!(
        corpus["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["input"].clone())
            .collect::<Vec<_>>(),
        vec![
            serde_json::json!([]),
            serde_json::json!([0]),
            serde_json::json!([0, 1, 15, 16, 127, 128, 254, 255])
        ],
        "helper independent byte input content"
    );

    assert_eq!(
        units(&corpus["noEncoder"]["error"]),
        "No UTF-8 encoder available in current runtime"
            .encode_utf16()
            .collect::<Vec<_>>(),
        "source unavailable encoder branch"
    );
    let string_inputs: Vec<Vec<u16>> = [
        "", "0x", "0xABcd", "0XAB", " x ", "é", "👩", "\u{feff}", "\u{85}",
    ]
    .into_iter()
    .map(|text| text.encode_utf16().collect())
    .chain([
        vec![0xd800],
        vec![0xdfff],
        vec![120, 0xd800, 121],
        vec![48, 120, 0xd800],
    ])
    .collect();
    let actual_inputs: Vec<_> = corpus["strings"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| units(&row["units"]))
        .collect();
    assert_eq!(
        actual_inputs, string_inputs,
        "helper independent UTF16 input content"
    );
    for (name, inputs) in [
        (
            "uint32",
            vec![
                "-0",
                "0",
                "1",
                "255",
                "4294967295",
                "4294967296",
                "-1",
                "1.5",
                "NaN",
                "Infinity",
                "-Infinity",
            ],
        ),
        (
            "int64",
            vec![
                "0",
                "1",
                "-1",
                "9223372036854775807",
                "-9223372036854775808",
                "9223372036854775808",
                "-9223372036854775809",
            ],
        ),
        (
            "classify",
            vec![
                "-1",
                "-0",
                "0",
                "1",
                "2",
                "3",
                "4",
                "5",
                "6",
                "2.5",
                "NaN",
                "Infinity",
                "-Infinity",
            ],
        ),
    ] {
        assert_eq!(
            corpus[name]
                .as_array()
                .unwrap()
                .iter()
                .map(|row| row["input"].as_str().unwrap())
                .collect::<Vec<_>>(),
            inputs,
            "helper independent {name} input content"
        );
    }
    let types = [
        None,
        Some("limit"),
        Some("market"),
        Some("stop_loss"),
        Some("stop_loss_limit"),
        Some("take_profit"),
        Some("take_profit_limit"),
    ];
    assert_eq!(
        corpus["orderTypes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|row| row["input"].as_str())
            .collect::<Vec<_>>(),
        types,
        "helper independent order type input content"
    );
    let combinations: Vec<_> = types
        .into_iter()
        .flat_map(|order| {
            [None, Some(false), Some(true)]
                .into_iter()
                .flat_map(move |post| {
                    [
                        None,
                        Some("GoodTillCancel"),
                        Some("ImmediateOrCancel"),
                        Some("FillOrKill"),
                        Some("PostOnly"),
                    ]
                    .into_iter()
                    .map(move |time| (order, post, time))
                })
        })
        .collect();
    let actual_combinations: Vec<_> = corpus["timeInForce"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| {
            (
                row["orderType"].as_str(),
                row["postOnly"].as_bool(),
                row["time"].as_str(),
            )
        })
        .collect();
    assert_eq!(
        actual_combinations, combinations,
        "helper independent time-in-force input content"
    );

    assert_eq!(
        corpus["strings"].as_array().unwrap().len(),
        13,
        "helper string inventory"
    );
    assert_eq!(
        corpus["fallback"].as_array().unwrap().len(),
        13,
        "helper fallback inventory"
    );
    for (index, row) in corpus["strings"].as_array().unwrap().iter().enumerate() {
        let input = units(&row["units"]);
        assert_eq!(
            utf8_to_hex(&input),
            row["encoded"].as_str().unwrap(),
            "utf8ToHex native bytes case {index}"
        );
        assert_eq!(
            utf8_to_hex(&input),
            corpus["fallback"][index].as_str().unwrap(),
            "utf8ToHex Buffer fallback {index}"
        );
        assert_eq!(
            to_hex_no_prefix(&input),
            units(&row["stripped"]),
            "toHexNoPrefix native UTF16 case {index}"
        );
    }
    assert_eq!(
        corpus["bytes"].as_array().unwrap().len(),
        3,
        "helper byte inventory"
    );
    for row in corpus["bytes"].as_array().unwrap() {
        let input = row["input"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| u8::try_from(n.as_u64().unwrap()).unwrap())
            .collect::<Vec<_>>();
        assert_eq!(
            bytes_to_hex(&input),
            row["output"].as_str().unwrap(),
            "bytesToHex native string"
        );
    }
    let field = "field".encode_utf16().chain([0xd800]).collect::<Vec<_>>();
    assert_eq!(
        corpus["uint32"].as_array().unwrap().len(),
        11,
        "helper uint32 inventory"
    );
    for row in corpus["uint32"].as_array().unwrap() {
        result(
            to_uint32_field_hex(number(row["input"].as_str().unwrap()), &field),
            &row["result"],
            "toUint32FieldHex",
        );
    }
    assert_eq!(
        corpus["int64"].as_array().unwrap().len(),
        7,
        "helper int64 inventory"
    );
    for row in corpus["int64"].as_array().unwrap() {
        result(
            to_int64_field_hex(
                &row["input"].as_str().unwrap().parse::<BigInt>().unwrap(),
                &field,
            ),
            &row["result"],
            "toInt64FieldHex",
        );
    }
    assert_eq!(
        corpus["orderTypes"].as_array().unwrap().len(),
        7,
        "helper order type inventory"
    );
    for row in corpus["orderTypes"].as_array().unwrap() {
        let input = optional(&row["input"]);
        assert_eq!(
            map_order_type_value(input.as_deref()).to_bits(),
            row["output"].as_f64().unwrap().to_bits(),
            "mapOrderTypeValue native number"
        );
    }
    assert_eq!(
        corpus["timeInForce"].as_array().unwrap().len(),
        105,
        "helper time-in-force inventory"
    );
    for row in corpus["timeInForce"].as_array().unwrap() {
        let order = optional(&row["orderType"]);
        let time = optional(&row["time"]);
        assert_eq!(
            map_time_in_force_value(order.as_deref(), row["postOnly"].as_bool(), time.as_deref())
                .to_bits(),
            row["output"].as_f64().unwrap().to_bits(),
            "mapTimeInForceValue native number {row}"
        );
    }
    assert_eq!(
        corpus["classify"].as_array().unwrap().len(),
        13,
        "helper classification inventory"
    );
    for row in corpus["classify"].as_array().unwrap() {
        let input = number(row["input"].as_str().unwrap());
        assert_eq!(
            is_stop_loss_order_type(input),
            row["stop"].as_bool().unwrap(),
            "isStopLossOrderType native bool"
        );
        assert_eq!(
            is_take_profit_order_type(input),
            row["take"].as_bool().unwrap(),
            "isTakeProfitOrderType native bool"
        );
        assert_eq!(
            is_child_grouped_order_type(input),
            row["child"].as_bool().unwrap(),
            "isChildGroupedOrderType native bool"
        );
    }
}
