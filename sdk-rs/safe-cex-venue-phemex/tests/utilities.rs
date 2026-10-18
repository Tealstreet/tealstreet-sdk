use safe_cex_types::PositionSide;
use safe_cex_venue_phemex::utilities::{calculate_price_diff, format_symbol_in, format_symbol_out};
use serde_json::Value;
use std::process::Command;

fn from_bits(value: &Value) -> f64 {
    let value = value.as_str().expect("bit string");
    if value == "NaN" {
        f64::NAN
    } else {
        f64::from_bits(u64::from_str_radix(value, 16).expect("binary64 bits"))
    }
}

#[test]
fn utilities_match_actual_typescript() {
    let output = Command::new("node")
        .arg(format!(
            "{}/tests/utility_cardinality_oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("one Node process per utility corpus");
    assert!(
        output.status.success(),
        "utility oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).expect("utility corpus JSON");
    eprintln!("utility oracle profile: {}", corpus["profile"]);
    assert_eq!(
        corpus["symbolInputs"].as_array().unwrap().len(),
        8,
        "PHEMEX-CARDINALITY symbol input total"
    );
    assert_eq!(
        corpus["symbolOutputs"].as_array().unwrap().len(),
        26,
        "PHEMEX-CARDINALITY symbol output total"
    );
    assert_eq!(
        corpus["numericOutputs"].as_array().unwrap().len(),
        6,
        "PHEMEX-CARDINALITY numeric output total"
    );
    let cases = corpus["cases"].as_array().expect("cases");
    assert_eq!(cases.len(), 722, "price diff corpus census");
    let mut comparisons = 0;
    for case_index in 0..722 {
        let case = corpus["cases"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed cases case");
        comparisons += 1;
        let side: PositionSide =
            serde_json::from_value(case["side"].clone()).expect("actual side enum");
        let actual =
            calculate_price_diff(from_bits(&case["price"]), from_bits(&case["mark"]), side);
        let expected = from_bits(&case["result"]);
        if expected.is_nan() {
            assert!(actual.is_nan(), "price difference native bits: {case}");
        } else {
            assert_eq!(
                actual.to_bits(),
                expected.to_bits(),
                "price difference native bits: {case}"
            );
        }
    }
    assert_eq!(
        corpus["upper"].as_array().unwrap().len(),
        1580,
        "PHEMEX-CARDINALITY uppercase mapping total"
    );
    let upper: std::collections::HashMap<u32, &str> = corpus["upper"]
        .as_array()
        .unwrap()
        .iter()
        .map(|row| (row[0].as_u64().unwrap() as u32, row[1].as_str().unwrap()))
        .collect();
    assert_eq!(
        upper.len(),
        1580,
        "PHEMEX-CARDINALITY unique uppercase mapping total"
    );
    let mut scalar_comparisons = 0;
    for cp in 0..=0x10ffff {
        if let Some(ch) = char::from_u32(cp) {
            scalar_comparisons += 1;
            let input = ch.to_string();
            assert_eq!(
                format_symbol_in(Some(&input)),
                upper.get(&cp).copied().unwrap_or(&input),
                "native symbol uppercase U+{cp:04X}"
            );
        }
    }
    assert_eq!(
        scalar_comparisons, 1112064,
        "PHEMEX-CARDINALITY scalar comparison total"
    );
    for case_index in 0..8 {
        let row = corpus["symbolInputs"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed symbolInputs case");
        comparisons += 1;
        assert_eq!(
            format_symbol_in(row["input"].as_str()),
            row["result"].as_str().unwrap(),
            "native symbol input fallback"
        );
    }
    assert_eq!(
        corpus["supplementOutputs"].as_array().unwrap().len(),
        2,
        "PHEMEX-CARDINALITY utility supplement total"
    );
    for (case_index, count) in [0usize, 3].into_iter().enumerate() {
        let row = corpus["supplementOutputs"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed supplementOutputs case");
        assert_eq!(
            row["result"].as_array().unwrap().len(),
            count,
            "PHEMEX-CARDINALITY utility source array length"
        );
    }
    for case_index in 0..28 {
        let row = if case_index < 26 {
            corpus["symbolOutputs"]
                .get(case_index)
                .expect("PHEMEX-CARDINALITY fixed symbolOutputs case")
        } else {
            corpus["supplementOutputs"]
                .get(case_index - 26)
                .expect("PHEMEX-CARDINALITY fixed supplementOutputs case")
        };
        comparisons += 1;
        let markets: Option<Vec<safe_cex_types::Market>> = if row["markets"].is_null() {
            None
        } else {
            Some(safe_cex_num::numeric_ingress::js_from_value(row["markets"].clone()).unwrap())
        };
        assert_eq!(
            format_symbol_out(markets.as_deref(), row["symbol"].as_str()),
            row["result"].clone().into(),
            "native symbol output first match: {row}"
        );
    }
    for case_index in 0..6 {
        let row = corpus["numericOutputs"]
            .get(case_index)
            .expect("PHEMEX-CARDINALITY fixed numericOutputs case");
        comparisons += 1;
        let market: safe_cex_types::Market =
            safe_cex_num::numeric_ingress::js_from_str(row["raw"].as_str().unwrap()).unwrap();
        let result = format_symbol_out(Some(&[market]), Some("BTC"));
        if row["type"] == "number" {
            assert!(
                result.as_f64().is_some(),
                "native symbol output numeric type"
            );
            assert_eq!(
                result.as_f64().unwrap().to_bits(),
                from_bits(&row["result"]).to_bits(),
                "native symbol output numeric bits"
            );
        } else {
            assert_eq!(
                result,
                row["result"].clone().into(),
                "native symbol output numeric fallback"
            );
        }
    }
    assert_eq!(
        comparisons, 764,
        "PHEMEX-CARDINALITY utility comparison total"
    );
}
