#[path = "support/number_bits.rs"]
mod capture;
use capture::number_bits;

#[test]
fn capture_canonicalizes_only_nan() {
    for bits in [
        0x7ff8_0000_0000_0000,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
    ] {
        assert_eq!(
            number_bits(f64::from_bits(bits)),
            "7ff8000000000000",
            "NaN payload canonicalization"
        );
    }
    assert_eq!(
        number_bits(-0.0),
        "8000000000000000",
        "signed zero remains bit exact"
    );
    for bits in [
        0_u64,
        1,
        0x3ff0_0000_0000_0001,
        0xbff0_0000_0000_0001,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
    ] {
        assert_eq!(
            number_bits(f64::from_bits(bits)),
            format!("{bits:016x}"),
            "non-NaN payload remains bit exact"
        );
    }
}

#[test]
fn javascript_capture_has_architecture_and_preserves_non_nan_bits() {
    let output = std::process::Command::new("node").arg("-e").arg(r#"
const bits=require(process.argv[1]);
const inputs=['7ff8000000000000','fff8000000000000','7ff0000000000001','8000000000000000','0000000000000000','0000000000000001','3ff0000000000001','bff0000000000001','7ff0000000000000','fff0000000000000'];
process.stdout.write(JSON.stringify({capture:{architecture:process.arch,platform:process.platform,node:process.version},bits:inputs.map(s=>bits(Buffer.from(s,'hex').readDoubleBE()))}));
"#).arg(format!("{}/scripts/number-bits.cjs", env!("CARGO_MANIFEST_DIR"))).output().unwrap();
    assert!(
        output.status.success(),
        "Number capture must execute: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let result: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(
        result["capture"]["architecture"].is_string(),
        "capture architecture required"
    );
    assert_eq!(
        result["bits"],
        serde_json::json!([
            "7ff8000000000000",
            "7ff8000000000000",
            "7ff8000000000000",
            "8000000000000000",
            "0000000000000000",
            "0000000000000001",
            "3ff0000000000001",
            "bff0000000000001",
            "7ff0000000000000",
            "fff0000000000000"
        ]),
        "source capture preserves signed zero and finite payloads"
    );
}
