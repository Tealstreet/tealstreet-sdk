#![cfg(all(
    feature = "secp256k1",
    feature = "ed25519",
    feature = "stark",
    feature = "schnorr"
))]

use safe_cex_crypto::{
    admission::{Family, PublicKey, Statement},
    signer::LocalSigner,
};
use safe_cex_engine::signing::{KeyRef, Provider};
use std::sync::Arc;

fn provider(family: Family, byte: u8) -> (Provider, KeyRef) {
    let width = if family == Family::Schnorr { 40 } else { 32 };
    let mut secret = vec![0; width];
    // Small valid scalars in either endian convention; fixed synthetic data.
    if family == Family::Schnorr {
        secret[0] = byte;
    } else {
        secret[31] = byte;
    }
    let (provider, ids) = LocalSigner::from_json_with_policy(
        serde_json::json!({"primary":{"key":"synthetic", "secret":safe_cex_crypto::hex(&secret)}}).to_string(),
        serde_json::from_value(serde_json::json!({"primary":{"secretEncoding":"hex", "signingAlgorithm":family.algorithm()}})).unwrap(),
    ).unwrap();
    (
        Arc::new(provider),
        ids.primary.unwrap().signing_ref().unwrap().clone(),
    )
}

fn statement(attempt: &str) -> Statement {
    Statement::new(attempt, [2; 32], [3; 32], 1000).unwrap()
}

#[test]
fn all_families_use_the_existing_provider_and_refuse_other_keys_and_statements() {
    for family in [
        Family::Ethereum,
        Family::Ed25519,
        Family::Stark,
        Family::Schnorr,
    ] {
        let (provider, reference) = provider(family, 7);
        let (other, other_ref) = self::provider(family, 8);
        let expected = PublicKey::from_provider(family, &provider, &reference).unwrap();
        let proof = statement("attempt-1");
        let signature = proof.sign(family, &provider, &reference, &[9; 32]).unwrap();
        assert!(expected.verify(&proof, &signature), "{family:?}");
        assert!(expected.matches_provider(&provider, &reference).unwrap());
        assert!(!expected.matches_provider(&other, &other_ref).unwrap());
        let wrong = proof.sign(family, &other, &other_ref, &[9; 32]).unwrap();
        assert!(!expected.verify(&proof, &wrong));
        for changed in [
            statement("attempt-2"),
            Statement::new("attempt-1", [4; 32], [3; 32], 1000).unwrap(),
            Statement::new("attempt-1", [2; 32], [4; 32], 1000).unwrap(),
            Statement::new("attempt-1", [2; 32], [3; 32], 1001).unwrap(),
        ] {
            assert!(!expected.verify(&changed, &signature), "{family:?}");
        }
        for malformed in [
            &[][..],
            &[0; 64][..],
            &[0; 80][..],
            &signature[..signature.len() - 1],
        ] {
            assert!(!expected.verify(&proof, malformed));
        }
        let mut altered = signature;
        altered[0] ^= 1;
        assert!(!expected.verify(&proof, &altered));
    }
}

#[test]
fn closed_statement_codec_and_non_transaction_messages() {
    let proof = statement("attempt-1");
    // Independent SHA-256 fixture over the specified binary field layout.
    assert_eq!(
        safe_cex_crypto::hex(&proof.digest()),
        "ad40a6f83222d4a33403234f3b12553dbfe7c0e60a00b7e3b71c76dbef5523a1"
    );
    let json = serde_json::to_string(&proof).unwrap();
    assert_eq!(Statement::from_json(&json).unwrap(), proof);
    assert!(Statement::from_json(&json.replace("tealstreet-admission-v1", "transaction")).is_err());
    assert!(Statement::from_json(&json.replace("attempt-1", "")).is_err());
    assert!(
        Statement::from_json(&json.replacen('{', "{\"url\":\"https://example.invalid\",", 1))
            .is_err()
    );
    assert!(Statement::from_json(&json.replacen('{', "{\"attempt\":\"duplicate\",", 1)).is_err());
    assert!(Statement::from_json(&format!("{json} null")).is_err());
    assert!(Statement::new(&"a".repeat(129), [0; 32], [0; 32], 0).is_err());
    assert!(Statement::new("a|b", [0; 32], [0; 32], 0).is_err());
    assert!(proof.current_at(1000, 15_000));
    assert!(proof.current_at(16_000, 15_000));
    assert!(!proof.current_at(16_001, 15_000));
    assert!(!proof.current_at(999, 15_000));
    assert!(!proof.current_at(u64::MAX, 15_000));
    let ed = proof.message(Family::Ed25519);
    assert!(ed.starts_with(b"tealstreet/admission/ed25519/v1\0"));
    assert!(!ed.starts_with(b"instruction="));
    assert!(serde_json::from_slice::<serde_json::Value>(&ed).is_err());
    let schnorr = proof.message(Family::Schnorr);
    assert_eq!(schnorr.len() % 8, 0);
    // Venue transactions start with a u32 chain ID in their first u64 word.
    assert!(u64::from_le_bytes(schnorr[..8].try_into().unwrap()) > u32::MAX as u64);
    assert_eq!(proof.message(Family::Ethereum).len(), 32);
    assert!(proof.message(Family::Stark)[0] < 4);
}

#[test]
fn public_keys_are_canonical_and_family_specific() {
    for family in [
        Family::Ethereum,
        Family::Ed25519,
        Family::Stark,
        Family::Schnorr,
    ] {
        assert!(PublicKey::new(family, &[]).is_err());
        let width = match family {
            Family::Ethereum => 20,
            Family::Schnorr => 40,
            _ => 32,
        };
        assert!(PublicKey::new(family, &vec![0; width]).is_err());
        assert!(PublicKey::new(family, &vec![255; width + 1]).is_err());
    }
    assert_eq!(Family::for_venue("hyperliquidspot"), Some(Family::Ethereum));
    assert_eq!(
        Family::for_venue("lighter-robinhood"),
        Some(Family::Schnorr)
    );
    assert_eq!(Family::for_venue("orderly"), Some(Family::Ed25519));
    assert_eq!(Family::for_venue("bybit"), None);
    assert_eq!(Family::for_venue("asterdex"), None); // v1 and v3 require explicit selection.
}

#[test]
fn ethereum_message_matches_independent_typed_data_encoder() {
    use alloy_dyn_abi::TypedData;
    let proof = statement("attempt-1");
    let typed: TypedData = serde_json::from_value(serde_json::json!({
        "types":{
            "EIP712Domain":[{"name":"name","type":"string"},{"name":"version","type":"string"}],
            "Admission":[{"name":"challenge","type":"bytes32"}]
        },
        "primaryType":"Admission", "domain":{"name":"Tealstreet Admission","version":"1"},
        "message":{"challenge":format!("0x{}",safe_cex_crypto::hex(&proof.digest()))}
    }))
    .unwrap();
    assert_eq!(
        proof.message(Family::Ethereum),
        typed.eip712_signing_hash().unwrap().as_slice()
    );
    assert_ne!(
        proof.message(Family::Ethereum),
        safe_cex_crypto::ethereum::ethereum_message_hash(&proof.digest())
    );
}
