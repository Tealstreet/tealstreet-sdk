#[path = "support/source_cardinality.rs"]
mod source_cardinality;
use safe_cex_num::{js_string, number_from_string, parse_float, parse_int};
use serde_json::{Value, json};
fn corpus(name: &str) -> Vec<Value> {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/p4_coercion_oracle.cjs"
        ))
        .arg(name)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    serde_json::from_slice(&output.stdout).unwrap()
}
fn equal_number(actual: f64, expected: &Value, row: &Value) {
    let expected = match expected.as_str().unwrap() {
        "NaN" => f64::NAN,
        "Infinity" => f64::INFINITY,
        "-Infinity" => f64::NEG_INFINITY,
        "-0" => -0.0,
        v => v.parse().unwrap(),
    };
    assert!(
        actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
        "{row}: actual {actual:?}, expected {expected:?}"
    );
}
#[test]
#[ignore = "requires root node dependencies"]
fn number_has_its_own_full_string_corpus() {
    source_cardinality::for_each(corpus("number"), 10018, "coercion-number", |row| {
        equal_number(
            number_from_string(row["text"].as_str().unwrap()),
            &row["expected"],
            &row,
        );
    });
}
#[test]
#[ignore = "requires root node dependencies"]
fn parse_float_has_its_own_prefix_corpus() {
    source_cardinality::for_each(corpus("float"), 10018, "coercion-float", |row| {
        equal_number(
            parse_float(row["text"].as_str().unwrap()),
            &row["expected"],
            &row,
        );
    });
}
#[test]
#[ignore = "requires root node dependencies"]
fn parse_int_has_its_own_radix_corpus() {
    source_cardinality::for_each(corpus("int"), 30264, "coercion-int", |row| {
        equal_number(
            parse_int(
                row["text"].as_str().unwrap(),
                Some(row["radix"].as_i64().unwrap() as i32),
            ),
            &row["expected"],
            &row,
        );
    });
}
#[test]
#[ignore = "requires root node dependencies"]
fn string_is_fallible_and_recursive() {
    source_cardinality::for_each(corpus("string"), 18, "coercion-string", |row| {
        let value = row["raw"]
            .as_str()
            .map(|raw| serde_json::from_str::<Value>(raw).unwrap());
        let result = match js_string(value.as_ref()) {
            Ok(text) => json!({"ok":text}),
            Err(error) => json!({"error":{"name":error.name,"message":error.message}}),
        };
        assert_eq!(result, row["expected"], "{row}");
    });
}
#[test]
#[ignore = "requires root node dependencies"]
fn actual_bitget_finite_or_zero_wrapper_preserves_the_primitive() {
    source_cardinality::for_each(corpus("measured"), 25, "coercion-measured", |row| {
        let value = &row["value"];
        let text = if value.is_null() {
            String::new()
        } else {
            js_string(Some(value)).unwrap()
        };
        let primitive = parse_float(&text);
        let wrapped = if primitive.is_finite() {
            primitive
        } else {
            0.0
        };
        equal_number(wrapped, &row["expected"], &row);
    });
    assert!(parse_float("").is_nan());
    assert!(parse_float("Infinity").is_infinite());
    assert_eq!(parse_float("-0").to_bits(), (-0.0_f64).to_bits());
    assert_eq!(
        parse_int(&js_string(Some(&json!(1e21))).unwrap(), Some(10)),
        1.0
    );
}
