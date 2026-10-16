#[path = "support/source_cardinality.rs"]
mod source_cardinality;
#[test]
fn pow_matches_live_node_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/js_pow.cjs"))
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<[String; 3]> = serde_json::from_slice(&output.stdout).unwrap();
    let mut mismatches = 0;
    source_cardinality::for_each(rows, 228973, "pow", |[input, power, expected]| {
        let x = f64::from_bits(u64::from_str_radix(&input, 16).unwrap());
        let y = f64::from_bits(u64::from_str_radix(&power, 16).unwrap());
        let expected = f64::from_bits(u64::from_str_radix(&expected, 16).unwrap());
        let actual = safe_cex_num::math_pow(x, y);
        if !(actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits()) {
            if mismatches < 10 {
                eprintln!(
                    "{x:?}^{y:?}: {:016x} != {:016x}",
                    actual.to_bits(),
                    expected.to_bits()
                );
            }
            mismatches += 1;
        }
    });
    assert_eq!(mismatches, 0, "Math.pow bit mismatches");
}
