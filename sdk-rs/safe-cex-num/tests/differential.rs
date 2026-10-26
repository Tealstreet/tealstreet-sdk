use safe_cex_num::*;
use serde_json::Value;

fn number(s: &str) -> f64 {
    match s {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        _ => s.parse().unwrap(),
    }
}

#[test]
#[ignore = "requires Node; run with --include-ignored for the differential gate"]
fn typescript_differential() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/gate.cjs"))
        .output()
        .expect("Node must be installed for the TypeScript differential gate");
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    // Pin corpus changes so shrinking the oracle requires an explicit reviewed edit.
    assert_eq!(cases.len(), 476_804);
    let mut disagreements = 0;
    for case in cases {
        let a = case["a"].as_str().map(number).unwrap_or(0.0);
        let b = case["b"].as_str().map(number).unwrap_or(0.0);
        let actual = match case["op"].as_str().unwrap() {
            "add" => add(a, b),
            "subtract" => subtract(a, b),
            "multiply" => multiply(a, b),
            "divide" => divide(a, b, case["precision"].as_str().map(number)),
            "adjust" => match &case["step"] {
                Value::String(s) => adjust(a, s.as_str()),
                Value::Number(n) => adjust(a, n.as_f64().unwrap()),
                _ => panic!("step must be number | string"),
            },
            "afterDecimalNumber" => after_decimal(a),
            "afterDecimal" => after_decimal(case["x"].as_str().unwrap()),
            _ => panic!("unknown case"),
        };
        let expected = number(case["expected"].as_str().unwrap());
        if let Some(observed) = case["observed"].as_array() {
            disagreements += 1;
            assert!(
                observed
                    .iter()
                    .any(|value| within_observed_ulp(actual, number(value.as_str().unwrap()))),
                "engine-disagreement case {case}: actual {actual:?} is outside observed values and their one-ULP neighbors"
            );
        } else {
            assert!(
                same_bits(actual, expected),
                "engine-agreement case {case}: actual {actual:?}, expected {expected:?}"
            );
        }
    }
    eprintln!(
        "numeric corpus: {} bit-exact agreement cases, {disagreements} engine-disagreement cases",
        476_804 - disagreements
    );
}

fn same_bits(actual: f64, expected: f64) -> bool {
    (actual.is_nan() && expected.is_nan()) || actual.to_bits() == expected.to_bits()
}

fn within_observed_ulp(actual: f64, observed: f64) -> bool {
    if same_bits(actual, observed) {
        return true;
    }
    // Nonfinite values and signed zero require an exact observed match.
    actual.is_finite()
        && observed.is_finite()
        && actual != 0.0
        && observed != 0.0
        && actual.is_sign_negative() == observed.is_sign_negative()
        && actual.to_bits().abs_diff(observed.to_bits()) <= 1
}

#[test]
fn observed_ulp_boundary() {
    let value = 2.675_f64;
    assert!(within_observed_ulp(
        f64::from_bits(value.to_bits() + 1),
        value
    ));
    assert!(!within_observed_ulp(
        f64::from_bits(value.to_bits() + 2),
        value
    ));
    assert!(within_observed_ulp(
        f64::from_bits((-value).to_bits() + 1),
        -value
    ));
    assert!(!within_observed_ulp(-0.0, 0.0));
    assert!(!within_observed_ulp(f64::MAX, f64::INFINITY));
    assert!(within_observed_ulp(f64::NAN, f64::NAN));
    assert!(!within_observed_ulp(1.0, f64::NAN));
}
