#[path = "support/source_cardinality.rs"]
mod source_cardinality;
#[test]
fn log10_matches_live_node_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_log10.cjs"))
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<[String; 2]> = serde_json::from_slice(&output.stdout).unwrap();
    source_cardinality::for_each(rows, 333938, "log10", |[input, expected]| {
        let x = f64::from_bits(u64::from_str_radix(&input, 16).unwrap());
        let expected = f64::from_bits(u64::from_str_radix(&expected, 16).unwrap());
        let actual = safe_cex_num::math_log10(x);
        assert!(
            actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
            "input {input} ({x:?}): {actual:?} != {expected:?}"
        );
    });
}
