#![cfg(not(target_arch = "wasm32"))]
use safe_cex_venue_grvt::{order_semantics::map_time_in_force_to_enum, types::GrvtTimeInForce};
#[test]
fn signing_tif_native_variant_identity() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/signing_tif.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    eprintln!("{}", String::from_utf8_lossy(&output.stderr));
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    let cases = [
        ("GOOD_TILL_TIME", GrvtTimeInForce::GoodTillTime),
        ("ALL_OR_NONE", GrvtTimeInForce::AllOrNone),
        ("IMMEDIATE_OR_CANCEL", GrvtTimeInForce::ImmediateOrCancel),
        ("FILL_OR_KILL", GrvtTimeInForce::FillOrKill),
    ];
    assert_eq!(rows.len(), cases.len(), "signing TIF census");
    for (key, variant) in cases {
        let row = rows.iter().find(|r| r["key"] == key).unwrap();
        assert_eq!(
            map_time_in_force_to_enum(variant).to_bits(),
            row["value"].as_f64().unwrap().to_bits(),
            "SIGNING TIF native variant identity {key}"
        );
    }
}
#[test]
fn zero_based_source_control_fails_by_name() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "signing_tif_native_variant_identity",
            "--nocapture",
        ])
        .env("GRVT_SIGNING_TIF_CONTROL", "zero-base")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(
        output.status.code(),
        Some(101),
        "source mutation assertion failure"
    );
    assert!(
        text.contains("SIGNING TIF native variant identity"),
        "named source control: {text}"
    );
}
