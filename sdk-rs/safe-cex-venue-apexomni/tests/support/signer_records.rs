use safe_cex_venue_apexomni::signer_records::{EthSignatureKind, TxEthSignature};
use serde::Deserialize;
use serde_json::{Value, json};
fn capture(mutation: bool) -> Value {
    let mut command = std::process::Command::new("node");
    command
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/scripts/signer-records.cjs"
        ))
        .env("SAFE_CEX_RUST_ARCH", std::env::consts::ARCH);
    if mutation {
        command.env("APEXOMNI_SIGNER_RECORD_MUTATION", "field");
    }
    let result = command
        .output()
        .expect("actual TypeScript compiler must run");
    assert!(
        result.status.success(),
        "signer record oracle: {}",
        String::from_utf8_lossy(&result.stderr)
    );
    serde_json::from_slice(&result.stdout).unwrap()
}
fn oracle() -> &'static Value {
    static CORPUS: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CORPUS.get_or_init(|| capture(false))
}
fn compare(corpus: &Value) {
    assert!(corpus["header"]["nodeMajor"].as_u64().unwrap() >= 18);
    assert!(corpus["header"]["nodeArch"].as_str().is_some());
    assert_eq!(corpus["header"]["rustArch"], std::env::consts::ARCH);
    assert_eq!(corpus["boundary"]["units"], json!([0xd800]));
    let raw = corpus["boundary"]["serialized"].as_str().unwrap();
    let value: TxEthSignature = serde_json::from_str(raw).unwrap();
    assert_eq!(
        value.signature, "\u{fffd}",
        "native signer String replacement"
    );
    typed_utf16::assert_projection(raw, &value);
    let mut fields = corpus["fields"].as_array().unwrap().clone();
    fields.sort_by_key(|f| f["name"].as_str().unwrap().to_owned());
    assert_eq!(
        fields,
        vec![
            json!({"name":"signature","optional":false,"kind":"string","literals":[]}),
            json!({"name":"type","optional":false,"kind":"union","literals":["EthereumSignature","EIP1271Signature"]}),
        ],
        "TxEthSignature actual source field inventory"
    );
    let cases = corpus["samples"].as_array().unwrap();
    assert_eq!(cases.len(), 10, "TxEthSignature source corpus total");
    let mut comparisons = 0;
    for row in cases {
        assert_eq!(row["typeType"], "string");
        assert_eq!(row["signatureType"], "string");
        let input = row["value"].clone();
        let raw = row["serialized"].as_str().unwrap();
        let owned: TxEthSignature = serde_json::from_value(input.clone()).unwrap();
        let decoded = [
            serde_json::from_str::<TxEthSignature>(raw).unwrap(),
            owned,
            TxEthSignature::deserialize(&input).unwrap(),
        ];
        for native in decoded {
            assert_eq!(
                serde_json::to_value(&native).unwrap(),
                input,
                "TxEthSignature emitted native fields"
            );
            let expected = match input["type"].as_str().unwrap() {
                "EthereumSignature" => EthSignatureKind::EthereumSignature,
                "EIP1271Signature" => EthSignatureKind::EIP1271Signature,
                _ => panic!("unclassified actual source kind"),
            };
            assert_eq!(native.kind, expected, "TxEthSignature native decoded kind");
            assert_eq!(
                native.signature,
                input["signature"].as_str().unwrap(),
                "TxEthSignature native decoded signature"
            );
            safe_cex_num::numeric_ingress::JsNumberOutput::check_number_output(&native);
            assert_eq!(
                serde_json::to_value(native).unwrap(),
                input,
                "TxEthSignature emitted native fields"
            );
            comparisons += 2;
        }
    }
    assert_eq!(
        comparisons, 60,
        "TxEthSignature executed native field comparisons"
    );
}
#[test]
fn signer_signature_matches_actual_typescript() {
    compare(oracle());
}
#[test]
fn signer_signature_rejects_invalid_shapes() {
    for value in [
        json!([]),
        json!(["EthereumSignature", "sig"]),
        json!({}),
        json!({"type":"EthereumSignature"}),
        json!({"signature":"sig"}),
        json!({"type":"other","signature":"sig"}),
        json!({"type":null,"signature":"sig"}),
        json!({"type":{"EthereumSignature":null},"signature":"sig"}),
        json!({"type":"EthereumSignature","signature":null}),
        json!({"type":"EthereumSignature","signature":7}),
    ] {
        assert!(
            serde_json::from_value::<TxEthSignature>(value.clone()).is_err(),
            "TxEthSignature strict shape {value}"
        );
        assert!(
            TxEthSignature::deserialize(&value).is_err(),
            "TxEthSignature borrowed strict shape {value}"
        );
        assert!(
            serde_json::from_str::<TxEthSignature>(&value.to_string()).is_err(),
            "TxEthSignature raw strict shape {value}"
        );
    }
    let raw = r#"{"type":"EthereumSignature","signature":"\ud800"}"#;
    typed_utf16::assert_projection(raw, &serde_json::from_str::<TxEthSignature>(raw).unwrap());
}

use crate::typed_utf16;
#[test]
fn signer_signature_source_and_cardinality_controls() {
    let error = std::panic::catch_unwind(|| compare(&capture(true)))
        .expect_err("added actual source field must fail");
    assert!(
        error
            .downcast_ref::<String>()
            .unwrap()
            .contains("TxEthSignature actual source field inventory")
    );
    for keep in [0, 9] {
        let mut corpus = oracle().clone();
        corpus["samples"].as_array_mut().unwrap().truncate(keep);
        let error =
            std::panic::catch_unwind(|| compare(&corpus)).expect_err("omitted cases must fail");
        assert!(
            error
                .downcast_ref::<String>()
                .unwrap()
                .contains("TxEthSignature source corpus total")
        );
    }
}
