#[test]
fn math_round_matches_live_node_bits() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/js_math_round.cjs"
        ))
        .output()
        .unwrap();
    assert!(output.status.success());
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<[String; 2]> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 20016);
    for [input, expected] in rows {
        let value = f64::from_bits(u64::from_str_radix(&input, 16).unwrap());
        let expected = f64::from_bits(u64::from_str_radix(&expected, 16).unwrap());
        let actual = safe_cex_num::math_round(value);
        assert!(
            actual.is_nan() && expected.is_nan() || actual.to_bits() == expected.to_bits(),
            "{input}: {actual:?} != {expected:?}"
        );
    }
    assert_eq!(safe_cex_num::display_round(f64::NAN), 0.0);
    assert!(safe_cex_num::math_round(f64::NAN).is_nan());
}

#[test]
fn number_capture_preserves_observable_bits() {
    let output = std::process::Command::new("node")
        .args([
            "--test",
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/support/number-bits.test.cjs"
            ),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "number capture controls: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}
