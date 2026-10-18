#![cfg(feature = "native")]
use safe_cex_venue_binance::{exchange_classes, native_host, private_session, request_weight};
#[allow(dead_code)]
#[path = "../scripts/sdk-call-observers.rs"]
mod observed;

#[test]
fn actual_sdk_candles_keep_native_fields_before_projection() {
    let source: Vec<serde_json::Value> =
        serde_json::from_str(include_str!("../fixtures/sdk-call-source.json")).unwrap();
    assert_eq!(source.len(), 6);
    for input in observed::inputs()
        .into_iter()
        .filter(|input| input.get("argumentError").is_none())
    {
        let actual = observed::public_sdk(&input);
        let expected = source
            .iter()
            .find(|row| row["input"] == input)
            .expect("authentic maintained source profile");
        assert_eq!(actual, expected["output"], "{input}");
        assert_eq!(actual["requests"].as_array().unwrap().len(), 1);
        assert_eq!(actual["result"]["candles"].as_array().unwrap().len(), 1);
        if input["profile"] == "nonfinite" {
            assert_eq!(
                actual["result"]["candles"][0]["volume"],
                serde_json::json!({"kind":"NaN"})
            );
        } else {
            assert_eq!(
                actual["result"]["candles"][0]["open"],
                serde_json::json!({"bits":"8000000000000000"})
            );
        }
    }
}
#[test]
#[ignore = "executes complete maintained SDK classes using isolated dependencies"]
fn maintained_sdk_candle_capture_is_current() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/sdk-call-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let rows: Vec<serde_json::Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(rows.len(), 6);
    for row in rows {
        assert_eq!(
            observed::public_sdk(&row["input"]),
            row["output"],
            "{}",
            row["input"]
        );
    }
}
