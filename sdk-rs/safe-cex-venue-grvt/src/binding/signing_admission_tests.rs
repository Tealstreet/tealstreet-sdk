//! Exercise the real constructed SDK host, with no replacement signer.
use super::*;
use crate::trading_runtime::TradingHost;
use safe_cex_exchange::js_runtime::value::{from_json, to_json};
use serde_json::{Value, json};

#[allow(dead_code)]
mod fixture {
    use crate as safe_cex_venue_grvt;
    include!(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/tests/support/binding_harness.rs"
    ));
}

#[test]
fn constructed_sdk_host_matches_builder_address_admission() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/binding-signing-oracle.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let source: Value = serde_json::from_slice(&output.stdout).unwrap();
    let cases = source["admissionCases"].as_array().unwrap();
    let inputs = [
        (
            "upper-prefix",
            json!("0XEE8F3559F298FFFA79B8253437969D15B5DCCFF3"),
        ),
        ("short-odd", json!("0x1")),
        ("short-even", json!("0x12")),
        ("short-mixed", json!("0xAbC")),
        ("zero-hex", json!("0x0")),
        ("unprefixed-decimal", json!("123")),
        (
            "unprefixed-hex",
            json!("ee8f3559f298fffa79b8253437969d15b5dccff3"),
        ),
        ("legacy-nonhex", json!("0xgg")),
        ("empty", json!("")),
        ("empty-prefix", json!("0x")),
        ("unicode", json!("😀")),
        ("legacy-long", json!("9".repeat(80))),
        ("number-zero", json!(0)),
        ("number-one", json!(1)),
        ("number-max-safe", json!(9007199254740991u64)),
        ("fraction", json!(1.5)),
        ("negative", json!(-1)),
        ("unsafe", json!(9007199254740992u64)),
        ("negative-text", json!("-1")),
        ("hex-too-long", json!(format!("0x{}", "a".repeat(41)))),
        ("null", Value::Null),
        ("boolean", json!(false)),
        ("array", json!([])),
        ("object", json!({})),
    ];
    assert_eq!(inputs.len(), 24);
    assert_eq!(cases.len(), inputs.len());
    let h = fixture::harness(ExchangeName::Grvt, fixture::options(), |_| {});
    let mut completed = 0;
    for ((name, builder), expected) in inputs.into_iter().zip(cases) {
        assert_eq!(expected["name"], name);
        assert_eq!(expected["builder"], builder, "independent input {name}");
        let mut typed = source["cases"][0]["typed"].clone();
        typed["message"]["builder"] = builder;
        let typed = from_json(&typed);
        let original = typed.get("message").unwrap().get("builder").unwrap();
        let result = h
            .exchange
            .rt
            .sdk_sign(from_json(&source["cases"][0]["key"]), typed.clone());
        assert!(
            typed
                .get("message")
                .unwrap()
                .get("builder")
                .unwrap()
                .strict_equals(&original),
            "SDK input value/reference must survive {name}"
        );
        if let Some(signature) = expected.get("signature") {
            let actual =
                result.unwrap_or_else(|error| panic!("GRVT SDK admission {name}: {error:?}"));
            assert_eq!(
                to_json(&actual).unwrap(),
                *signature,
                "SDK signature {name}"
            );
        } else {
            let error = result.expect_err("SDK refusal");
            for key in ["name", "message"] {
                assert_eq!(
                    to_json(&error.get(key).unwrap()).unwrap(),
                    expected["error"][key],
                    "SDK error {name}/{key}"
                );
            }
        }
        completed += 1;
    }
    assert_eq!(
        completed, 24,
        "every input was compared through the constructed host"
    );
    h.exchange.dispose().unwrap();
}
