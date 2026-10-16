use safe_cex_num::numeric_ingress::{js_from_str, js_from_value, js_from_value_ref};
use safe_cex_types::Ticker;
use serde_json::Value;
use std::process::Command;

#[test]
fn ticker_timestamp_retains_actual_clock_value_and_numeric_entrypaths() {
    let capture = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../safe-cex-types/tests/ticker_timestamp_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        capture.status.success(),
        "{}",
        String::from_utf8_lossy(&capture.stderr)
    );
    let capture: Value = serde_json::from_slice(&capture.stdout).unwrap();
    let rows = capture["rows"].as_array().unwrap();
    assert_eq!(rows.len(), 3, "TICKER-TIMESTAMP source total");
    let expected = [(123.25_f64, 41.5_f64), (-0.0, 53.5), (987.5, 67.5)];
    let mut comparisons = 0;
    for index in 0..3 {
        let (timestamp, volume) = expected[index];
        assert_eq!(
            rows[index]["timestampBits"],
            format!("{:016x}", timestamp.to_bits())
        );
        let mut value: Value = serde_json::from_str(rows[index]["wire"].as_str().unwrap()).unwrap();
        // Restore the observed native clock event before the JSON projection erased -0.
        value["timestamp"] = serde_json::Number::from_f64(timestamp).unwrap().into();
        let native: Ticker = serde_json::from_value(value.clone()).unwrap();
        assert_eq!(
            native.timestamp.unwrap().to_bits(),
            timestamp.to_bits(),
            "TICKER-TIMESTAMP native clock identity"
        );
        assert_eq!(
            native.quote_volume.to_bits(),
            volume.to_bits(),
            "TICKER-TIMESTAMP native quoteVolume identity"
        );
        let canonical: Ticker = js_from_value(value).unwrap();
        assert_eq!(canonical.timestamp.unwrap().to_bits(), timestamp.to_bits());
        comparisons += 2;
    }
    assert_eq!(
        comparisons, 6,
        "TICKER-TIMESTAMP executed clock comparisons"
    );
    let base = rows[0]["wire"].as_str().unwrap();
    let spellings = [
        ("-0", -0.0_f64),
        ("-0.0", -0.0),
        ("1e400", f64::INFINITY),
        ("-1e400", f64::NEG_INFINITY),
        ("1e3", 1000.0),
        ("1.5", 1.5),
    ];
    let mut numeric_comparisons = 0;
    for (lexeme, expected) in spellings {
        let raw = base.replace("\"timestamp\":123.25", &format!("\"timestamp\":{lexeme}"));
        let value: Value = serde_json::from_str(&raw).unwrap();
        let natives: [Ticker; 3] = [
            js_from_str(&raw).unwrap(),
            js_from_value(value.clone()).unwrap(),
            js_from_value_ref(&value).unwrap(),
        ];
        for native in natives {
            assert_eq!(
                native.timestamp.unwrap().to_bits(),
                expected.to_bits(),
                "TICKER-TIMESTAMP canonical numeric bits {lexeme}"
            );
            numeric_comparisons += 1;
        }
    }
    assert_eq!(
        numeric_comparisons, 18,
        "TICKER-TIMESTAMP numeric comparison count"
    );
    let mut absent: Value = serde_json::from_str(base).unwrap();
    absent.as_object_mut().unwrap().remove("timestamp");
    let native: Ticker = js_from_value(absent.clone()).unwrap();
    assert_eq!(native.timestamp, None, "TICKER-TIMESTAMP absence identity");
    assert!(
        !serde_json::to_value(native)
            .unwrap()
            .as_object()
            .unwrap()
            .contains_key("timestamp")
    );
    absent["timestamp"] = Value::Null;
    assert!(
        js_from_value::<Ticker>(absent).is_err(),
        "TICKER-TIMESTAMP present null refusal"
    );
}
