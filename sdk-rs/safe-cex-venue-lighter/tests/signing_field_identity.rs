//! Later live supplement; the independent 92-case fixture is never rewritten.
use safe_cex_crypto::lighter::{
    SchnorrNonce, poseidon2_hash, public_key, schnorr_sign, schnorr_verify,
};
use serde_json::Value;
use std::process::Command;
fn capture() -> Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let output = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/scripts/capture-signing.cjs"
                ))
                .env("LIGHTER_SIGNING_IDENTITY_SUPPLEMENT", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            let mut data: Value = serde_json::from_slice(&output.stdout).unwrap();
            match std::env::var("LIGHTER_SIGNING_CARDINALITY_MUTATION").as_deref() {
                Ok("duplicate-case") => data["cases"][1] = data["cases"][0].clone(),
                Ok("move-signature") => {
                    let call = data["cases"][0]["crypto"]
                        .as_array_mut()
                        .unwrap()
                        .pop()
                        .unwrap();
                    data["cases"][1]["crypto"]
                        .as_array_mut()
                        .unwrap()
                        .push(call);
                }
                Ok("empty") => data["cases"].as_array_mut().unwrap().clear(),
                Ok("subset") => {
                    data["cases"].as_array_mut().unwrap().pop().unwrap();
                }
                _ => {}
            }
            data
        })
        .clone()
}
fn decode(text: &str) -> Vec<u8> {
    assert_eq!(text.len() % 2, 0);
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
fn hex(bytes: &[u8]) -> String {
    bytes.iter().map(|b| format!("{b:02x}")).collect()
}
#[test]
#[ignore = "requires Node, maintained TypeScript and repository Go WASM"]
fn signing_field_identity_supplement_matches_shared_crypto() {
    let data = capture();
    assert_eq!(
        data["cases"].as_array().unwrap().len(),
        24,
        "supplement case inventory"
    );
    let mut expected_names = Vec::new();
    for exchange in ["lighter", "lighter-robinhood"] {
        for network in ["livenet", "testnet"] {
            for operation in [
                "approve-distinct-fees",
                "approve-distinct-fees-l1",
                "approve-field-maxPerpsTakerFee",
                "approve-field-maxPerpsMakerFee",
                "approve-field-maxSpotTakerFee",
                "approve-field-maxSpotMakerFee",
            ] {
                expected_names.push(format!("{exchange}/{network}/{operation}"));
            }
        }
    }
    assert_eq!(
        data["cases"]
            .as_array()
            .unwrap()
            .iter()
            .map(|case| case["name"].as_str().unwrap())
            .collect::<Vec<_>>(),
        expected_names,
        "SIGNING-CARDINALITY fixed case identities"
    );
    let key = decode(data["secret"].as_str().unwrap());
    let pk = public_key(&key).unwrap();
    let mut count = 0;
    for case in data["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        assert_eq!(
            case["crypto"]
                .as_array()
                .unwrap()
                .iter()
                .map(|call| call["method"].as_str().unwrap())
                .collect::<Vec<_>>(),
            ["poseidon2Hash", "schnorrSign"],
            "SIGNING-CARDINALITY {name} per-case hash/sign calls"
        );
        for call in case["crypto"].as_array().unwrap() {
            count += 1;
            let input = decode(call["inputs"][0].as_str().unwrap());
            let result = match call["method"].as_str().unwrap() {
                "poseidon2Hash" => hex(&poseidon2_hash(&input).unwrap()),
                "schnorrSign" => {
                    let signature =
                        schnorr_sign(&input, &key, SchnorrNonce::from_bytes(&[66; 40]).unwrap())
                            .unwrap();
                    assert!(
                        schnorr_verify(&input, &pk, &signature).unwrap(),
                        "{name} supplement signature verifies"
                    );
                    hex(&signature)
                }
                method => panic!("unclassified supplement method {method}"),
            };
            assert_eq!(
                result,
                call["result"].as_str().unwrap(),
                "{name} field-identity {}",
                call["method"]
            );
        }
    }
    assert_eq!(count, 48, "supplement WASM call inventory");
}
#[test]
#[ignore = "requires Node, maintained TypeScript and repository Go WASM"]
fn signing_field_identity_preimages_retain_each_fee() {
    let data = capture();
    assert!(
        data["captureHeader"]["captureArchitecture"]
            .as_str()
            .is_some_and(|value| !value.is_empty()),
        "capture architecture provenance"
    );
    assert_eq!(data["cases"].as_array().unwrap().len(), 24);
    for case in data["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let fee_values: std::collections::BTreeSet<_> = [
            "maxPerpsTakerFee",
            "maxPerpsMakerFee",
            "maxSpotTakerFee",
            "maxSpotMakerFee",
        ]
        .iter()
        .map(|field| case["outer"]["args"][0][field].as_u64().unwrap())
        .collect();
        assert_eq!(fee_values.len(), 4, "{name} discriminating fee inventory");
        let hash_calls: Vec<_> = case["crypto"]
            .as_array()
            .unwrap()
            .iter()
            .filter(|call| call["method"] == "poseidon2Hash")
            .collect();
        assert_eq!(hash_calls.len(), 1, "{name} hash boundary inventory");
        let preimage = decode(hash_calls[0]["inputs"][0].as_str().unwrap());
        assert_eq!(preimage.len(), 96, "{name} preimage width");
        for (index, field) in [
            "maxPerpsTakerFee",
            "maxPerpsMakerFee",
            "maxSpotTakerFee",
            "maxSpotMakerFee",
        ]
        .iter()
        .enumerate()
        {
            let offset = (7 + index) * 8;
            let actual = u64::from_le_bytes(preimage[offset..offset + 8].try_into().unwrap());
            assert_eq!(
                actual,
                case["outer"]["args"][0][field].as_u64().unwrap(),
                "{name}.{field} preimage identity"
            );
        }
    }
}
#[test]
#[ignore = "requires Node, maintained TypeScript and repository Go WASM"]
fn signing_field_identity_source_swap_fails_by_name() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "signing_field_identity_preimages_retain_each_fee",
            "--ignored",
            "--nocapture",
        ])
        .env("LIGHTER_SIGNING_IDENTITY_MUTATION", "swap-fees")
        .output()
        .unwrap();
    let log = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(output.status.code(), Some(101), "{log}");
    assert!(
        log.contains("lighter/livenet/approve-distinct-fees.maxPerpsMakerFee preimage identity"),
        "{log}"
    );
}

#[test]
#[ignore = "requires Node, maintained TypeScript and repository Go WASM"]
fn signing_cardinality_controls_fail_by_name() {
    for (mutation, assertion) in [
        (
            "duplicate-case",
            "SIGNING-CARDINALITY fixed case identities",
        ),
        ("move-signature", "per-case hash/sign calls"),
        ("empty", "supplement case inventory"),
        ("subset", "supplement case inventory"),
    ] {
        let output = Command::new(std::env::current_exe().unwrap())
            .args([
                "--exact",
                "signing_field_identity_supplement_matches_shared_crypto",
                "--ignored",
                "--nocapture",
            ])
            .env("LIGHTER_SIGNING_CARDINALITY_MUTATION", mutation)
            .output()
            .unwrap();
        let log = format!(
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        assert_eq!(output.status.code(), Some(101), "{log}");
        assert!(log.contains(assertion), "{log}");
    }
}
