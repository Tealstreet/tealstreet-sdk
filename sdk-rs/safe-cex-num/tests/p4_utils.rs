#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::*;
use serde_json::Value;
fn number(value: &Value) -> f64 {
    value.as_str().unwrap().parse().unwrap_or(f64::NAN)
}
#[test]
#[ignore = "executes live TypeScript numeric utilities"]
fn p4_numeric_typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/p4-oracle.cjs"))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(cases, 10565, "p4-utils", |case| {
        let input = case["input"].as_str().map(PrecisionInput::from);
        let op = case["op"].as_str().unwrap();
        let actual = match op {
            "multiply1" => multiply1(number(&case["a"]), number(&case["b"])),
            "divide1" => divide1(number(&case["a"]), number(&case["b"])),
            "handleRoundingError" => handle_rounding_error(number(&case["a"])),
            "round" => round(number(&case["a"])),
            "roundUSD" => round_usd(number(&case["a"])),
            "countDecimals" => ccxt_precision::count_decimals(input),
            "decimalToPrecision" => ccxt_precision::decimal_to_precision(input),
            "ccxtParse" => {
                ccxt_precision::parse_precision(input, case["integer"].as_bool().unwrap())
            }
            "integerPrecisionToAmount" => {
                assert_eq!(
                    ccxt_precision::integer_precision_to_amount(input)
                        .as_deref()
                        .unwrap_or("undefined"),
                    case["expected"].as_str().unwrap(),
                    "{case}"
                );
                return;
            }
            "parsePrecision" => {
                let result = parse_precision(case["input"].as_str());
                if let Some(error) = case["error"].as_str() {
                    assert_eq!(result.unwrap_err(), error);
                } else {
                    assert_eq!(
                        result.unwrap().as_deref(),
                        case["expected"].as_str(),
                        "{case}"
                    );
                }
                return;
            }
            "precisionToString" => {
                assert_eq!(
                    ccxt_precision::precision_to_string(number(&case["a"])),
                    case["expected"].as_str().unwrap(),
                    "{case}"
                );
                return;
            }
            _ => panic!("{case}"),
        };
        let expected = number(&case["expected"]);
        assert!(
            if expected.is_nan() {
                actual.is_nan()
            } else {
                actual.to_bits() == expected.to_bits()
            },
            "{case}: actual {actual:?}"
        );
    });
}
#[test]
fn original_precision_loss_and_binary_rounding_are_preserved() {
    assert_eq!(multiply1(0.01, 0.01), 0.0);
    assert_eq!(divide1(1.0, 3.0), 0.0);
    assert_eq!(handle_rounding_error(1.00000000005), 1.0000000001);
    assert_eq!(round_usd(-0.005).to_bits(), (-0.0_f64).to_bits());
    assert_eq!(parse_precision(Some("2tail")).unwrap(), Some("0.01".into()));
}
