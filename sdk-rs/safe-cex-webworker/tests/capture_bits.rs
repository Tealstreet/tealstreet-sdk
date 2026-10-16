#[path = "support/capture_bits.rs"]
mod capture;

#[test]
fn capture_canonicalizes_only_nan_preserving_zero_sign_and_finite_payloads() {
    let output = std::process::Command::new("node")
        .args(["--input-type=module", "-e"])
        .arg(concat!(
            "import {captureBits,captureArchitecture,captureNodeMajor} from './sdk-rs/safe-cex-webworker/tests/capture_bits.mjs';",
            "const bytes=new DataView(new ArrayBuffer(8));",
            "const tokens=['7ff8000000000000','fff8000000000000','7ff0000000000001','fff8000000000042','8000000000000000','0000000000000000','3ff4000000000000','bff4000000000000','0000000000000001','7ff0000000000000','fff0000000000000'];",
            "console.log(JSON.stringify({architecture:captureArchitecture,nodeMajor:captureNodeMajor,rows:tokens.map(token=>{bytes.setBigUint64(0,BigInt('0x'+token));return [token,captureBits(bytes.getFloat64(0))]})}));"
        ))
        .current_dir(concat!(env!("CARGO_MANIFEST_DIR"),"/../.."))
        .output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert!(source["architecture"].as_str().unwrap().contains('/'));
    assert!(
        source["nodeMajor"]
            .as_u64()
            .is_some_and(|major| major >= 20),
        "CAPTURE Node major provenance"
    );
    let tokens = [
        0x7ff8_0000_0000_0000_u64,
        0xfff8_0000_0000_0000,
        0x7ff0_0000_0000_0001,
        0xfff8_0000_0000_0042,
        0x8000_0000_0000_0000,
        0,
        0x3ff4_0000_0000_0000,
        0xbff4_0000_0000_0000,
        1,
        0x7ff0_0000_0000_0000,
        0xfff0_0000_0000_0000,
    ];
    let rows = source["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 11, "CAPTURE-CARDINALITY source total");
    let mut comparisons = 0;
    for (index, raw) in tokens.into_iter().enumerate() {
        let row = rows
            .get(index)
            .expect("CAPTURE-CARDINALITY missing independent token")
            .as_array()
            .unwrap();
        assert_eq!(row.len(), 2, "CAPTURE-CARDINALITY token/output pair");
        assert_eq!(
            u64::from_str_radix(row[0].as_str().unwrap(), 16).unwrap(),
            raw,
            "CAPTURE-IDENTITY independent token"
        );
        let value = f64::from_bits(raw);
        let expected = if value.is_nan() {
            0x7ff8_0000_0000_0000
        } else {
            raw
        };
        let name = if value.is_nan() {
            "CAPTURE canonical NaN"
        } else if value == 0.0 {
            "CAPTURE signed-zero identity"
        } else {
            "CAPTURE finite and infinity payload identity"
        };
        assert_eq!(
            u64::from_str_radix(row[1].as_str().unwrap(), 16).unwrap(),
            expected,
            "{name} source"
        );
        assert_eq!(capture::capture_bits(value), expected, "{name} Rust");
        comparisons += 1;
    }
    assert_eq!(comparisons, 11, "CAPTURE-CARDINALITY native comparisons");
}
