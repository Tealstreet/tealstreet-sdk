use safe_cex_num::numeric_ingress::js_deserialize_passthrough;
use safe_cex_venue_paradex::exchange_helpers::*;
use serde_json::{Value, json};
use std::process::Command;

fn from_bits(value: &Value) -> f64 {
    f64::from_bits(u64::from_str_radix(value.as_str().unwrap(), 16).unwrap())
}
fn bits(n: f64) -> String {
    format!(
        "{:016x}",
        if n.is_nan() {
            0x7ff8000000000000
        } else {
            n.to_bits()
        }
    )
}
#[test]
fn actual_typescript_exchange_helpers() {
    let out = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/exchange_helpers_oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "source_execution {}",
        String::from_utf8_lossy(&out.stderr)
    );
    let corpus: Value = serde_json::from_slice(&out.stdout).unwrap();
    let arch = match std::env::consts::ARCH {
        "aarch64" => "arm64",
        "x86_64" => "x64",
        s => s,
    };
    assert_eq!(corpus["header"]["arch"], arch, "capture_cpu_architecture");
    assert!(
        corpus["header"]["nodeMajor"].as_u64().unwrap() > 0,
        "capture_node_major"
    );
    println!("capture_header {}", corpus["header"]);
    for (key, count) in [("scalar", 33), ("rounding", 416), ("intervals", 32)] {
        assert_eq!(
            corpus[key].as_array().unwrap().len(),
            count,
            "helper_corpus_census {key}"
        );
    }
    let mut compared = [0; 5];
    for c in corpus["scalar"].as_array().unwrap() {
        let n = c["numberBits"]
            .as_str()
            .map(|_| from_bits(&c["numberBits"]));
        assert_eq!(
            is_positive_finite_price(n),
            c["positive"].as_bool().unwrap(),
            "positive_price_native {}",
            c["label"]
        );
        compared[0] += 1;
        // Native NaN is outside the JSON-shaped unknown boundary. Observe its TS
        // outcome separately and assert retained ingress refuses its construction.
        if n.is_some_and(f64::is_nan) {
            assert_eq!(
                c["fraction"],
                json!({"ok":true,"value":{"type":"undefined"}})
            );
            assert!(
                safe_cex_types::retained_json::RetainedJsonInput::Float(f64::NAN)
                    .into_value()
                    .is_err(),
                "chosen_nan_json_refusal"
            );
            compared[2] += 1;
            continue;
        }
        let input: Option<Value> = c["raw"].as_str().map(|raw| {
            js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(raw)).unwrap()
        });
        let observed = match normalize_positive_fraction(input.as_ref()) {
            Ok(Some(n)) => json!({"ok":true,"value":{"type":"number","bits":bits(n)}}),
            Ok(None) => json!({"ok":true,"value":{"type":"undefined"}}),
            Err(e) => json!({"ok":false,"name":e.name,"message":e.message}),
        };
        assert_eq!(
            observed, c["fraction"],
            "positive_fraction_native {}",
            c["label"]
        );
        compared[1] += 1;
    }
    for c in corpus["rounding"].as_array().unwrap() {
        let value = round_price_to_step(
            from_bits(&c["price"]),
            from_bits(&c["step"]),
            c["direction"].as_str().unwrap(),
        );
        assert_eq!(
            bits(value),
            c["result"],
            "round_price_native price={} step={} direction={}",
            c["price"],
            c["step"],
            c["direction"]
        );
        compared[3] += 1;
    }
    for c in corpus["intervals"].as_array().unwrap() {
        let observed =
            match convert_interval_to_trading_view_resolution(c["input"].as_str().unwrap()) {
                Ok(s) => json!({"ok":true,"value":{"type":"string","value":s}}),
                Err(message) => json!({"ok":false,"name":"Error","message":message}),
            };
        assert_eq!(
            observed, c["result"],
            "interval_resolution_native {}",
            c["input"]
        );
        compared[4] += 1;
    }
    assert_eq!(
        compared,
        [33, 32, 1, 416, 32],
        "helper_completed_comparisons"
    );
    assert_eq!(corpus["surrogate"]["intervalErrorCodeUnit"], 0xd800);
    assert_eq!(
        corpus["surrogate"]["fraction"],
        json!({"ok":true,"value":{"type":"undefined"}})
    );
    assert!(
        js_deserialize_passthrough(&mut serde_json::Deserializer::from_str(
            corpus["surrogate"]["raw"].as_str().unwrap()
        ))
        .is_err(),
        "chosen_utf8_surrogate_refusal"
    );
}
