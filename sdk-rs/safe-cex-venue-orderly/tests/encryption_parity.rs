use safe_cex_venue_orderly::encryption::{base64url, orderly_sign, sha512_for_ed25519};
use serde_json::Value;
use std::process::Command;

#[test]
fn actual_typescript_encryption_corpus() {
    let output = Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/encryption-oracle.cjs"
        ))
        .output()
        .expect("start one Node encryption corpus");
    assert!(
        output.status.success(),
        "actual encryption oracle: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let corpus: Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(
        corpus["hashes"].as_array().unwrap().len(),
        8,
        "ORDERLY-CARDINALITY encryption/hashes source total"
    );
    assert_eq!(
        corpus["encodings"].as_array().unwrap().len(),
        8,
        "ORDERLY-CARDINALITY encryption/encodings source total"
    );
    assert_eq!(
        corpus["signatures"].as_array().unwrap().len(),
        83,
        "ORDERLY-CARDINALITY encryption/signatures source total"
    );
    eprintln!("encryption runtime: {}", corpus["runtime"]);
    let mut compared_hashes = 0;
    for (index, row) in corpus["hashes"].as_array().unwrap().iter().enumerate() {
        let bytes: Vec<u8> = row["bytes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|v| u8::try_from(v.as_u64().unwrap()).unwrap())
            .collect();
        let actual: String = sha512_for_ed25519(&bytes)
            .iter()
            .map(|b| format!("{b:02x}"))
            .collect();
        assert_eq!(
            actual,
            row["digest"].as_str().unwrap(),
            "sha512ForEd25519/digest/{index}"
        );
        compared_hashes += 1;
    }
    assert_eq!(compared_hashes, 8, "ORDERLY-COMPARISONS encryption/hashes");
    let mut compared_encodings = 0;
    for (index, row) in corpus["encodings"].as_array().unwrap().iter().enumerate() {
        assert_eq!(
            base64url(row["input"].as_str().unwrap()),
            row["output"].as_str().unwrap(),
            "base64url/text/{index}"
        );
        compared_encodings += 1;
    }
    assert_eq!(
        compared_encodings, 8,
        "ORDERLY-COMPARISONS encryption/encodings"
    );
    let mut compared_signatures = 0;
    for (index, row) in corpus["signatures"].as_array().unwrap().iter().enumerate() {
        let actual = orderly_sign(
            row["text"].as_str().unwrap(),
            row["secret"].as_str().unwrap(),
        );
        if let Some(signature) = row["signature"].as_str() {
            assert_eq!(
                actual.as_deref(),
                Ok(signature),
                "orderlySign/signature/{index}"
            );
        } else {
            let error = actual.expect_err("source signing refusal must not succeed");
            assert_eq!(
                error.name,
                row["error"]["name"].as_str().unwrap(),
                "orderlySign/error-name/{index}"
            );
            assert_eq!(
                error.message,
                row["error"]["message"].as_str().unwrap(),
                "orderlySign/error-message/{index}"
            );
        }
        compared_signatures += 1;
    }
    assert_eq!(
        compared_signatures, 83,
        "ORDERLY-COMPARISONS encryption/signatures"
    );
    assert_eq!(
        corpus["surrogate"]["units"],
        serde_json::json!([0xd800]),
        "source lone surrogate captured before JSON"
    );
    assert_eq!(
        corpus["surrogate"]["replacementSignature"],
        corpus["surrogate"]["explicitReplacementSignature"],
        "source TextEncoder surrogate replacement"
    );
    assert!(
        String::from_utf16(&[0xd800]).is_err(),
        "chosen UTF-8 input refusal"
    );
    assert_eq!(
        corpus["surrogate"]["base64Units"],
        serde_json::json!([0xd800, 0x2d, 0x5f]),
        "base64url source surrogate preservation"
    );
    assert_eq!(
        orderly_sign("\u{fffd}", corpus["surrogate"]["secret"].as_str().unwrap()).unwrap(),
        corpus["surrogate"]["replacementSignature"]
            .as_str()
            .unwrap(),
        "TextEncoder explicit replacement signature"
    );
    let fixed = orderly_sign("hook", corpus["hook"]["secret"].as_str().unwrap()).unwrap();
    assert_eq!(
        fixed,
        corpus["hook"]["normal"].as_str().unwrap(),
        "fixed shared SHA512 signature"
    );
    assert_ne!(
        fixed,
        corpus["hook"]["changed"].as_str().unwrap(),
        "chosen mutable hash hook omission"
    );
}
