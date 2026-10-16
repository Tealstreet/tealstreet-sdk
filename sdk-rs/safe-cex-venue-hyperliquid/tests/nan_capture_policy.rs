fn capture_bits(value: f64) -> u64 {
    if value.is_nan() {
        0x7ff8000000000000
    } else {
        value.to_bits()
    }
}
#[test]
fn signed_zero_capture_is_bit_exact() {
    assert_eq!(
        capture_bits(-0.0),
        0x8000000000000000,
        "signed-zero capture must retain negative zero"
    );
    assert_eq!(
        capture_bits(0.0),
        0,
        "signed-zero capture must retain positive zero"
    );
}
#[test]
fn finite_capture_is_bit_exact() {
    assert_eq!(
        capture_bits(1.25),
        0x3ff4000000000000,
        "finite capture payload must remain exact"
    );
    assert_eq!(
        capture_bits(-3.5),
        0xc00c000000000000,
        "negative finite capture payload must remain exact"
    );
}
#[test]
fn nan_capture_normalizes_only_nan() {
    let fixture: serde_json::Value =
        serde_json::from_str(include_str!("fixtures/nan-capture-profile.json")).unwrap();
    assert_eq!(fixture["captureArchitecture"], "arm64");
    for raw in [0x7ff8000000000000, 0xfff8000000000000, 0x7ff0000000000001] {
        assert_eq!(
            capture_bits(f64::from_bits(raw)),
            0x7ff8000000000000,
            "NaN-only capture normalization"
        );
    }
    assert_eq!(capture_bits(f64::INFINITY), f64::INFINITY.to_bits());
}

#[test]
fn actual_node_capture_keeps_non_nan_bits() {
    let script = concat!(env!("CARGO_MANIFEST_DIR"), "/scripts/number-bits.cjs");
    let code = format!(
        "const bits=require({:?}); console.log([NaN,-0,0,1.25,-3.5,Infinity].map(bits).join(','));",
        script
    );
    let output = std::process::Command::new("node")
        .args(["-e", &code])
        .output()
        .unwrap();
    assert!(output.status.success());
    assert_eq!(
        String::from_utf8(output.stdout).unwrap().trim(),
        "7ff8000000000000,8000000000000000,0000000000000000,3ff4000000000000,c00c000000000000,7ff0000000000000",
        "actual capture NaN-only policy, signed-zero and finite identities"
    );
}
