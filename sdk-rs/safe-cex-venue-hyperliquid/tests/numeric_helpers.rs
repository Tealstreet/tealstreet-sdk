use safe_cex_num::{js_number, js_number_string};
use safe_cex_venue_hyperliquid::exchange_helpers::*;
use serde_json::Value;

#[test]
fn actual_exchange_numeric_helpers_pin_pre_serialization_values() {
    let mut cases: Vec<Value> =
        serde_json::from_str(include_str!("fixtures/numeric-helpers.json")).unwrap();
    cases.extend(
        serde_json::from_str::<Vec<Value>>(include_str!("fixtures/numeric-string-refusals.json"))
            .unwrap(),
    );
    for operation in [
        "parseFiniteNumber",
        "calculatePriceChangePercentage",
        "isFinitePositiveNumber",
    ] {
        assert!(cases.iter().any(|row| row["operation"] == operation));
    }
    for raw in [
        r#"["0x10"]"#,
        r#"["12abc"]"#,
        r#"["\ufeff12tail"]"#,
        r#"["\u008512"]"#,
        r#"[{"toString":null}]"#,
        "[1e400]",
    ] {
        assert!(
            cases
                .iter()
                .any(|row| row["operation"] == "parseFiniteNumber" && row["raw"] == raw),
            "numeric corpus property {raw}"
        );
    }
    assert!(cases.iter().any(|row| row["numberString"] == "Infinity"));
    assert!(cases.iter().any(|row| row["numberString"] == "-Infinity"));
    assert!(cases.iter().any(|row| row["representationRefusal"] == true));
    for operation in ["parseFiniteNumber", "isFinitePositiveNumber"] {
        for raw in [r#"["\ud800"]"#, r#"["\udfff12"]"#] {
            assert!(
                cases.iter().any(|row| row["operation"] == operation
                    && row["raw"] == raw
                    && row["representationRefusal"] == true),
                "every divergent helper requires both lone-surrogate witnesses: {operation} {raw}"
            );
        }
    }
    assert!(cases.iter().any(|row| row["raw"] == r#"["😀12"]"#));
    for row in cases {
        let raw = row["raw"].as_str().unwrap();
        if row["representationRefusal"] == true {
            assert!(
                serde_json::from_str::<Vec<Value>>(raw).is_err(),
                "shared UTF-8 ingress refuses lone surrogates from valid JSON escapes"
            );
            if row["operation"] == "isFinitePositiveNumber" {
                assert_eq!(
                    row["value"], false,
                    "TypeScript accepts the preserved raw witness"
                );
            } else {
                assert_eq!(
                    row["numberString"], "0",
                    "TypeScript accepts the preserved raw witness"
                );
            }
            continue;
        }
        let args: Vec<Value> = serde_json::from_str(raw).unwrap();
        if row["operation"] == "isFinitePositiveNumber" {
            assert_eq!(
                Value::Bool(is_finite_positive_number(&args[0]).unwrap()),
                row["value"],
                "{row}"
            );
            continue;
        }
        let result = match row["operation"].as_str().unwrap() {
            "parseFiniteNumber" => parse_finite_number(&args[0]),
            "calculatePriceChangePercentage" => Ok(calculate_price_change_percentage(
                js_number(args.first()).unwrap(),
                js_number(args.get(1)).unwrap(),
            )),
            other => panic!("unrecognized numeric oracle operation {other}"),
        };
        match result {
            Ok(value) => {
                assert!(
                    row.get("error").is_none(),
                    "source refusal must remain an error: {row}"
                );
                assert_eq!(
                    js_number_string(value),
                    row["numberString"].as_str().unwrap(),
                    "pre-serialization numeric value: {row}"
                );
            }
            Err(error) => {
                assert_eq!(
                    error.name,
                    row["errorName"].as_str().unwrap(),
                    "error identity: {row}"
                );
                assert_eq!(error.message, row["error"].as_str().unwrap(), "{row}");
            }
        }
    }
}

#[test]
fn numeric_oracle_executes_actual_source_and_fails_its_mutation() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/numeric-helpers.cjs");
    let output = std::process::Command::new("node")
        .arg(script)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let boundaries = std::process::Command::new("node")
        .args([script, "--refusals"])
        .output()
        .unwrap();
    assert!(
        boundaries.status.success(),
        "{}",
        String::from_utf8_lossy(&boundaries.stderr)
    );
    let control = std::process::Command::new("node")
        .args([script, "--control"])
        .output()
        .unwrap();
    assert!(
        !control.status.success(),
        "actual TypeScript parseFloat-to-Number mutation must fail"
    );
    assert!(
        String::from_utf8_lossy(&control.stderr).contains("actual exchange numeric helper drift"),
        "mutation must fail at the oracle assertion"
    );
}
