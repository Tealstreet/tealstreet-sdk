use safe_cex_num::numeric_ingress::JsNumberInput;
use safe_cex_venue_mexc::types::{
    MexcCode, MexcEnvelope, decode_mexc_envelope, decode_mexc_envelope_value,
};
use serde_json::{Value, json};

fn code_bits<T>(envelope: &MexcEnvelope<T>) -> String {
    match envelope.code.as_ref().unwrap() {
        MexcCode::Number(n) => format!("{:016x}", n.to_bits()),
        MexcCode::String(_) => panic!("numeric source must not become a string"),
    }
}
fn checked_code_bits<T>(envelope: &MexcEnvelope<T>, lexeme: &str) -> String {
    let bits = code_bits(envelope);
    match (
        std::env::var("MEXC_NUMERIC_INGRESS_RUST_CONTROL").as_deref(),
        lexeme,
    ) {
        (Ok("collapse-signed-zero"), "-0") => "0000000000000000".to_owned(),
        (Ok("change-finite"), "1.5") => format!("{:016x}", 2.5_f64.to_bits()),
        _ => bits,
    }
}
fn data_bits(envelope: &MexcEnvelope) -> String {
    format!(
        "{:016x}",
        safe_cex_num::js_number(envelope.data.as_ref())
            .unwrap()
            .to_bits()
    )
}
fn input_data_bits(envelope: &MexcEnvelope<JsNumberInput>) -> String {
    format!(
        "{:016x}",
        envelope
            .data
            .as_ref()
            .unwrap()
            .clone()
            .decode::<f64>()
            .unwrap()
            .to_bits()
    )
}

#[test]
fn mexc_envelope_numeric_ingress_matches_node() {
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/numeric_ingress_oracle.mjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let cases: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(cases.len(), 9);
    let mut raw_refusals = 0;
    let mut value_refusals = 0;
    for case in &cases {
        assert!(
            case["captureArchitecture"]
                .as_str()
                .is_some_and(|arch| !arch.is_empty())
        );
        let lexeme = case["lexeme"].as_str().unwrap();
        let data_lexeme = case["dataLexeme"].as_str().unwrap();
        let raw = case["raw"].as_str().unwrap();
        let parsed: Value = serde_json::from_str(raw).unwrap();
        assert_eq!(
            format!(
                "{:016x}",
                safe_cex_num::js_number(parsed.get("code"))
                    .unwrap()
                    .to_bits()
            ),
            case["codeBits"],
            "input lexeme {lexeme}"
        );
        let checked_raw = decode_mexc_envelope::<JsNumberInput>(raw)
            .expect("checked raw envelope accepts Node numeric spelling");
        assert_eq!(
            checked_code_bits(&checked_raw, lexeme),
            case["codeBits"],
            "checked raw {lexeme}"
        );
        assert_eq!(
            input_data_bits(&checked_raw),
            case["dataBits"],
            "checked raw data {lexeme}"
        );
        let checked_value = decode_mexc_envelope_value::<JsNumberInput>(parsed.clone())
            .expect("checked existing-Value envelope accepts Node numeric spelling");
        assert_eq!(
            checked_code_bits(&checked_value, lexeme),
            case["codeBits"],
            "checked Value {lexeme}"
        );
        assert_eq!(
            input_data_bits(&checked_value),
            case["dataBits"],
            "checked Value data {lexeme}"
        );
        let direct = serde_json::from_str::<MexcEnvelope<Value>>(raw);
        let existing = serde_json::from_value::<MexcEnvelope<Value>>(parsed);
        // Keep the old decoder's failures as regression evidence while the
        // checked whole-entry path below proves the shared repair matches Node.
        if matches!(lexeme, "-0" | "-0.0" | "1.5" | "1e3" | "1e400" | "-1e400") {
            assert!(
                direct
                    .unwrap_err()
                    .to_string()
                    .contains("untagged enum MexcCode"),
                "raw refusal {lexeme}"
            );
            raw_refusals += 1;
        } else {
            let envelope = direct.unwrap();
            assert_eq!(code_bits(&envelope), case["codeBits"], "raw {lexeme}");
            assert_eq!(data_bits(&envelope), case["dataBits"], "raw data {lexeme}");
        }
        if matches!(lexeme, "1e3" | "1e400" | "-1e400") {
            assert!(
                existing
                    .unwrap_err()
                    .to_string()
                    .contains("untagged enum MexcCode"),
                "Value refusal {lexeme}"
            );
            value_refusals += 1;
        } else {
            let envelope = existing.unwrap();
            if lexeme == "-0" {
                assert_eq!(case["codeBits"], "8000000000000000");
                assert_eq!(
                    code_bits(&envelope),
                    "0000000000000000",
                    "FINDING existing-Value code lexeme loses negative zero"
                );
            } else {
                assert_eq!(code_bits(&envelope), case["codeBits"], "Value {lexeme}");
            }
            if data_lexeme == "-0" {
                assert_eq!(case["dataBits"], "8000000000000000");
                assert_eq!(
                    data_bits(&envelope),
                    "0000000000000000",
                    "FINDING existing-Value data lexeme loses negative zero"
                );
            } else {
                assert_eq!(
                    data_bits(&envelope),
                    case["dataBits"],
                    "Value data {lexeme}"
                );
            }
        }
        // Unknown data has its own path, independently of the code union refusal.
        let data_raw = format!(r#"{{"data":{data_lexeme}}}"#);
        let direct: MexcEnvelope<Value> = serde_json::from_str(&data_raw).unwrap();
        assert_eq!(
            data_bits(&direct),
            case["dataBits"],
            "raw unknown data {data_lexeme}"
        );
        let existing_data =
            serde_json::from_value::<MexcEnvelope<Value>>(serde_json::from_str(&data_raw).unwrap())
                .unwrap();
        if data_lexeme == "-0" {
            assert_eq!(
                data_bits(&existing_data),
                "0000000000000000",
                "FINDING existing-Value data integer lexeme loses negative zero"
            );
        } else {
            assert_eq!(
                data_bits(&existing_data),
                case["dataBits"],
                "existing-Value data-only {data_lexeme}"
            );
        }
    }
    assert_eq!(raw_refusals, 6);
    assert_eq!(value_refusals, 3);
    let native_code_zero: MexcEnvelope =
        serde_json::from_value(json!({"code":-0.0,"data":1.5})).unwrap();
    assert_eq!(code_bits(&native_code_zero), "8000000000000000");
    assert_eq!(
        data_bits(&native_code_zero),
        format!("{:016x}", 1.5_f64.to_bits())
    );
    let native_data_zero: MexcEnvelope =
        serde_json::from_value(json!({"code":1.5,"data":-0.0})).unwrap();
    assert_eq!(
        code_bits(&native_data_zero),
        format!("{:016x}", 1.5_f64.to_bits())
    );
    assert_eq!(data_bits(&native_data_zero), "8000000000000000");
}

#[test]
fn mexc_numeric_ingress_bit_controls_fail_by_named_case() {
    for (control, assertion) in [
        ("collapse-signed-zero", "checked raw -0"),
        ("change-finite", "checked raw 1.5"),
    ] {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "mexc_envelope_numeric_ingress_matches_node",
                "--nocapture",
            ])
            .env("MEXC_NUMERIC_INGRESS_RUST_CONTROL", control)
            .output()
            .unwrap();
        let text = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{control}: {text}");
        assert!(
            text.contains("mexc_envelope_numeric_ingress_matches_node ... FAILED"),
            "{control}: {text}"
        );
        assert!(text.contains(assertion), "{control}: {text}");
    }
}

#[test]
fn mexc_numeric_ingress_typescript_control_is_observed() {
    let output = std::process::Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "mexc_envelope_numeric_ingress_matches_node",
            "--nocapture",
        ])
        .env("MEXC_NUMERIC_INGRESS_TS_CONTROL", "remove-number")
        .output()
        .unwrap();
    let text = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{text}");
    assert!(
        text.contains("mexc_envelope_numeric_ingress_matches_node ... FAILED")
            && text.contains("optional code union"),
        "{text}"
    );
}
