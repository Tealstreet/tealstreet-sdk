//! Later live supplement; the independent 92-case fixture is never rewritten.
use safe_cex_crypto::lighter::{
    SchnorrNonce, poseidon2_hash, public_key, schnorr_sign, schnorr_verify,
};
use serde_json::Value;
use std::process::Command;
#[path = "support/lighter_source_pin.rs"]
mod source_pin;
fn capture() -> Value {
    static CAPTURE: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
    CAPTURE
        .get_or_init(|| {
            let output = Command::new("node")
                .arg(concat!(
                    env!("CARGO_MANIFEST_DIR"),
                    "/scripts/lighter-field-identities.cjs"
                ))
                .env("LIGHTER_SIGNING_IDENTITY_SUPPLEMENT", "1")
                .output()
                .unwrap();
            assert!(
                output.status.success(),
                "{}",
                String::from_utf8_lossy(&output.stderr)
            );
            use sha2::{Digest, Sha256};
            assert_eq!(
                hex(&Sha256::digest(include_bytes!(
                    "../scripts/lighter-field-identities.cjs"
                ))),
                "99a7a0b050d6b86f7a74c1b6072fd93ed76035712e89ffa956f5518b27473cba",
                "adopted generator revision pin"
            );
            let data: Value = serde_json::from_slice(&output.stdout).unwrap();
            let mut normalized = data.clone();
            let pins: Value = serde_json::from_str(include_str!(
                "fixtures/lighter/reviewed-source-provenance.json"
            ))
            .unwrap();
            for (file, pin) in pins.as_object().unwrap() {
                if pin.get("declarationsSha256").is_some() {
                    source_pin::verify(file, Some(normalized["sources"][file].as_str().unwrap()));
                } else {
                    assert_eq!(
                        normalized["sources"][file], pin["currentSha256"],
                        "unreviewed independent source drift: {file}"
                    );
                }
                normalized["sources"][file] = pin["originalSha256"].clone();
            }
            if std::env::var_os("LIGHTER_SIGNING_IDENTITY_MUTATION").is_none() {
                if data["captureHeader"]["nodeVersion"] == "v26.7.0"
                    && data["captureHeader"]["captureArchitecture"] == "arm64"
                    && data["captureHeader"]["capturePlatform"] == "darwin"
                {
                    assert_eq!(
                        hex(&Sha256::digest(format!(
                            "{}\n",
                            serde_json::to_string_pretty(&normalized).unwrap()
                        ))),
                        "fe35cc51111d42ce88c35268a5ae105783a661a7101855822b91285b35f1d12f",
                        "original fixed capture SHA after reviewed provenance normalization on its recorded runtime"
                    );
                }
                let mut semantic = normalized;
                for key in ["nodeVersion", "captureArchitecture", "capturePlatform"] {
                    semantic["captureHeader"]
                        .as_object_mut()
                        .unwrap()
                        .shift_remove(key);
                }
                assert_eq!(
                    hex(&Sha256::digest(serde_json::to_vec(&semantic).unwrap())),
                    "260c28f1e11f4c5e73b1aad0e9ca5c28e7d809a7a3cb700e02a993dcffcc8f52",
                    "original supplement semantic identity after reviewed provenance normalization, excluding three retained runtime labels"
                );
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
fn signing_field_identity_supplement_matches_shared_crypto() {
    let data = capture();
    assert_eq!(
        data["cases"].as_array().unwrap().len(),
        24,
        "supplement case inventory"
    );
    let key = decode(data["secret"].as_str().unwrap());
    let pk = public_key(&key).unwrap();
    let mut count = 0;
    for case in data["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        for call in case["crypto"].as_array().unwrap() {
            count += 1;
            let input = decode(call["inputs"][0].as_str().unwrap());
            let result = match call["method"].as_str().unwrap() {
                "poseidon2Hash" => poseidon2_hash(&input).unwrap().to_vec(),
                "schnorrSign" => {
                    let signature =
                        schnorr_sign(&input, &key, SchnorrNonce::from_bytes(&[66; 40]).unwrap())
                            .unwrap();
                    assert!(
                        schnorr_verify(&input, &pk, &signature).unwrap(),
                        "{name} supplement signature verifies"
                    );
                    signature.to_vec()
                }
                method => panic!("unclassified supplement method {method}"),
            };
            assert_eq!(
                result,
                decode(call["result"].as_str().unwrap()),
                "{name} field-identity {}",
                call["method"]
            );
        }
    }
    assert_eq!(count, 48, "supplement WASM call inventory");
}
#[test]
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
fn signing_field_identity_source_swap_fails_by_name() {
    let output = Command::new(std::env::current_exe().unwrap())
        .args([
            "--exact",
            "signing_field_identity_preimages_retain_each_fee",
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
