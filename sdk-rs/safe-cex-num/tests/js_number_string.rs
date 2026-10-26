use safe_cex_num::js_number_string;
use std::io::Write;
use std::process::{Command, Stdio};

#[test]
fn number_spelling_matches_live_node_at_boundaries_and_binary64_samples() {
    let mut values = vec![
        0.0,
        -0.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
        f64::MAX,
        f64::from_bits(1),
        42.0,
        9007199254740992.0,
    ];
    for boundary in [1e-7_f64, 1e-6, 1e20, 1e21] {
        for bits in [
            boundary.to_bits() - 1,
            boundary.to_bits(),
            boundary.to_bits() + 1,
        ] {
            values.extend([f64::from_bits(bits), -f64::from_bits(bits)]);
        }
    }
    let mut bits = 0x123456789abcdef0_u64;
    for _ in 0..10_000 {
        bits = bits.wrapping_mul(6364136223846793005).wrapping_add(1);
        values.push(f64::from_bits(bits));
    }
    let input = serde_json::to_vec(
        &values
            .iter()
            .map(|v| format!("{:016x}", v.to_bits()))
            .collect::<Vec<_>>(),
    )
    .unwrap();
    let mut child = Command::new("node")
        .args([
            "-e",
            r#"
const fs = require('node:fs');
const bits = JSON.parse(fs.readFileSync(0,'utf8'));
console.log(JSON.stringify(bits.map(hex => {
  const b = Buffer.alloc(8); b.writeBigUInt64LE(BigInt('0x'+hex));
  return String(b.readDoubleLE());
})));
"#,
        ])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("Node required");
    child.stdin.take().unwrap().write_all(&input).unwrap();
    let output = child.wait_with_output().unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let mut expected: Vec<String> = serde_json::from_slice(&output.stdout).unwrap();
    let control = std::env::var("NUM_SPELLING_CAPTURE_CONTROL").unwrap_or_default();
    match control.as_str() {
        "empty" => expected.clear(),
        "subset" => expected.truncate(1),
        _ => {}
    }
    assert_eq!(expected.len(), 10_034, "NUMBER-SPELLING source total");
    let native_count = values.len();
    assert_eq!(native_count, 10_034, "NUMBER-SPELLING native input total");
    if control == "after-source-count" {
        expected.clear();
    }
    let mut compared = 0;
    for (value, expected) in values.into_iter().zip(expected) {
        compared += 1;
        assert_eq!(
            js_number_string(value),
            expected,
            "binary64 {:016x}",
            value.to_bits()
        );
    }
    assert_eq!(
        compared, native_count,
        "NUMBER-SPELLING compared/native count"
    );
}

#[test]
fn truncated_capture_controls_fail_by_name() {
    for (control, assertion) in [
        ("empty", "NUMBER-SPELLING source total"),
        ("subset", "NUMBER-SPELLING source total"),
        (
            "after-source-count",
            "NUMBER-SPELLING compared/native count",
        ),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "number_spelling_matches_live_node_at_boundaries_and_binary64_samples",
                "--nocapture",
            ])
            .env("NUM_SPELLING_CAPTURE_CONTROL", control)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(
            output.status.code(),
            Some(101),
            "capture control {control}: {log}"
        );
        assert!(
            log.contains(assertion),
            "named capture control {control}: {log}"
        );
    }
}
