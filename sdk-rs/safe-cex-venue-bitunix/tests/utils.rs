use safe_cex_venue_bitunix::utils::sha256_hex;
use serde_json::Value;
use std::process::Command;

#[test]
fn actual_typescript_sha256_hex() {
    let result = Command::new("node")
        .arg(format!(
            "{}/tests/utils-oracle.cjs",
            env!("CARGO_MANIFEST_DIR")
        ))
        .output()
        .expect("execute actual TypeScript utility");
    assert!(
        result.status.success(),
        "actual TS utility: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    eprintln!(
        "oracle runtime: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    let rows: Vec<Value> = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(rows.len(), 25, "utility corpus census");
    let mut refused = 0;
    let mut compared = 0;
    for row in rows {
        let units: Vec<u16> = serde_json::from_value(row["units"].clone()).unwrap();
        match String::from_utf16(&units) {
            Ok(text) => {
                compared += 1;
                assert_eq!(
                    sha256_hex(&text),
                    row["digest"].as_str().unwrap(),
                    "utility sha256Hex {}",
                    row["name"]
                );
            }
            Err(_) => {
                refused += 1;
                // Source TextEncoder replaces unpaired surrogates. Observe its
                // digest, but do not silently admit that replacement at ingress.
                assert_eq!(
                    sha256_hex(&String::from_utf16_lossy(&units)),
                    row["digest"].as_str().unwrap(),
                    "utility surrogate replacement observation"
                );
            }
        }
    }
    assert_eq!(compared, 21, "SHA native comparison count");
    assert_eq!(refused, 4, "utility UTF-8 boundary refusals");
}
