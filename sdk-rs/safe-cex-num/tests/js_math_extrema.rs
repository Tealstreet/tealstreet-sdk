#[test]
fn math_min_max_match_live_node_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_math_extrema.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<[String; 4]> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 63121);
    for [left, right, min, max] in rows {
        let decode = |s: &str| f64::from_bits(u64::from_str_radix(s, 16).unwrap());
        let a = decode(&left);
        let b = decode(&right);
        for (name, actual, expected) in [
            ("Math.min", safe_cex_num::math_min(a, b), decode(&min)),
            ("Math.max", safe_cex_num::math_max(a, b), decode(&max)),
        ] {
            assert!(
                actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
                "{name}({left}, {right}): {:016x} != {:016x}",
                actual.to_bits(),
                expected.to_bits()
            );
        }
    }
}
