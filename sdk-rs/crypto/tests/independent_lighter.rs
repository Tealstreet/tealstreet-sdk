//! Lighter Poseidon2/Schnorr vectors captured from the venue's Go-WASM signer.
use safe_cex_crypto::lighter::{
    SchnorrNonce, poseidon2_hash_fp5, public_key, schnorr_sign, schnorr_verify,
};
use serde_json::Value;
use std::collections::BTreeMap;
const FIXTURE: &str = include_str!("fixtures/lighter/venue-signing-preimages.json");
fn decode(text: &str) -> Vec<u8> {
    assert!(text.len().is_multiple_of(2));
    text.as_bytes()
        .as_chunks::<2>()
        .0
        .iter()
        .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
        .collect()
}
#[test]
fn retained_wasm_fp5_and_signature_vectors_match_rust() {
    let corpus: Value = serde_json::from_str(FIXTURE).unwrap();
    let key = decode(corpus["secret"].as_str().unwrap());
    let pk = public_key(&key).unwrap();
    assert_eq!(pk.as_slice(), decode(corpus["publicKey"].as_str().unwrap()));
    assert_eq!(corpus["entropyByte"], 66);
    let mut calls = BTreeMap::<String, usize>::new();
    let mut profiles = BTreeMap::<String, usize>::new();
    for case in corpus["cases"].as_array().unwrap() {
        let name = case["name"].as_str().unwrap();
        let profile = name.split('/').take(2).collect::<Vec<_>>().join("/");
        *profiles.entry(profile).or_default() += 1;
        for call in case["crypto"].as_array().unwrap() {
            let method = call["method"].as_str().unwrap();
            // The exchange uses Fp5 signing; other captured hashes stay historical.
            if matches!(method, "poseidon2HashTwoToOne" | "poseidon2Hash") {
                continue;
            }
            *calls.entry(method.into()).or_default() += 1;
            let input = decode(call["inputs"][0].as_str().unwrap());
            let expected = call["result"].as_str().unwrap();
            let result = match method {
                "poseidon2HashFp5" => poseidon2_hash_fp5(&input).unwrap().to_vec(),
                "schnorrSign" => {
                    // Offline fixture ONLY: Go rand.Int's big-endian 0x42 bytes are symmetric and canonical.
                    let nonce = SchnorrNonce::from_bytes(&[66; 40]).unwrap();
                    let signature = schnorr_sign(&input, &key, nonce).unwrap();
                    assert!(
                        schnorr_verify(&input, &pk, &decode(expected)).unwrap(),
                        "independent signature verification: {name}"
                    );
                    signature.to_vec()
                }
                other => panic!("uncompared independent WASM method: {other}"),
            };
            assert_eq!(
                result,
                decode(expected),
                "independent WASM mismatch: {name}/{method}"
            );
        }
    }
    assert_eq!(
        calls,
        BTreeMap::from([("poseidon2HashFp5".into(), 60), ("schnorrSign".into(), 92)])
    );
    assert_eq!(
        profiles,
        BTreeMap::from([
            ("lighter/livenet".into(), 23),
            ("lighter/testnet".into(), 23),
            ("lighter-robinhood/livenet".into(), 23),
            ("lighter-robinhood/testnet".into(), 23)
        ])
    );
}
