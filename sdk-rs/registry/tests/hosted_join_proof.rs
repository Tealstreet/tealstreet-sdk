#![cfg(feature = "hosted-join-proof")]

use safe_cex_crypto::{
    admission::{
        Family, PublicKey, Statement,
        identity::Venue,
        wire::{Request, Response},
    },
    signer::LocalSigner,
};
use safe_cex_engine::signing::{Identities, Identity, KeyRef, Operation, Provider, SignResult};
use safe_cex_venues::hosted_join_proof::Account;
use std::sync::Arc;

const NOW: u64 = 1_700_000_000_000;
fn options() -> safe_cex_venues::EngineOptions {
    serde_json::from_str("{}").unwrap()
}

fn fixture(venue: &str, family: Family) -> (Account, Provider, PublicKey, String, Statement) {
    let mut signer = LocalSigner::new();
    let key = KeyRef("owned-primary-signing".into());
    let mut scalar = vec![0; if family == Family::Schnorr { 40 } else { 32 }];
    if family == Family::Schnorr {
        scalar[0] = 7;
    } else {
        scalar[31] = 7;
    }
    signer
        .insert_declared(key.clone(), scalar, family.algorithm())
        .unwrap();
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("owned-public-credential".into()),
            Some(key.clone()),
        )),
        secondary: None,
    };
    let provider: Provider = Arc::new(signer);
    let public = PublicKey::from_provider(family, &provider, &key).unwrap();
    let account = Account::capture(venue, &ids, &options()).unwrap();
    let statement = Statement::new("proof-attempt", [1; 32], [2; 32], NOW).unwrap();
    let venue: Venue = serde_json::from_value(venue.into()).unwrap();
    let request = Request::new(
        "wire-correlation",
        venue,
        "account",
        "registration",
        statement.clone(),
    )
    .unwrap();
    (
        account,
        provider,
        public,
        serde_json::to_string(&request).unwrap(),
        statement,
    )
}

fn prove(venue: &str, family: Family) {
    let (account, provider, public, raw, statement) = fixture(venue, family);
    let (work, pending) = account.prepare(&raw, NOW, &[9; 32]).unwrap();
    assert_eq!(
        work.operations.len(),
        1,
        "no credential export or extra signing work"
    );
    let Operation::Sign { algorithm, key, .. } = &work.operations[0] else {
        panic!("only the owned signing operation is allowed")
    };
    assert_eq!(*algorithm, family.algorithm());
    assert_eq!(key, &KeyRef("owned-primary-signing".into()));
    let result = provider.sign_ready(&work).unwrap().unwrap();
    work.validate_result(&result).unwrap();
    let response = pending.finish(&result).unwrap();
    assert_eq!(response.id(), "wire-correlation");
    let raw = serde_json::to_string(&response).unwrap();
    let decoded = Response::from_json(&raw, family).unwrap();
    assert!(public.verify(&statement, &decoded.signature().unwrap()));
    let changed = Statement::new("another-attempt", [1; 32], [2; 32], NOW).unwrap();
    assert!(!public.verify(&changed, &decoded.signature().unwrap()));
    assert_eq!(
        serde_json::from_str::<serde_json::Value>(&raw)
            .unwrap()
            .as_object()
            .unwrap()
            .len(),
        2
    );
}

#[test]
fn ed25519_device_challenge_uses_owned_key_and_frozen_output() {
    prove("backpack", Family::Ed25519);
}
#[test]
fn ethereum_device_challenge_uses_admission_domain() {
    prove("hyperliquid", Family::Ethereum);
}
#[test]
fn grvt_device_challenge_proves_the_separate_ecdsa_scalar() {
    prove("grvt", Family::Ethereum);
}

#[test]
fn stark_device_challenge_uses_admission_domain() {
    prove("paradex", Family::Stark);
}
#[test]
fn schnorr_device_challenge_uses_admission_domain() {
    prove("lighter", Family::Schnorr);
}

#[test]
fn malformed_or_foreign_challenges_refuse_before_provider_work() {
    let (account, _, _, raw, _) = fixture("backpack", Family::Ed25519);
    let input: serde_json::Value = serde_json::from_str(&raw).unwrap();
    for field in [
        "id",
        "type",
        "exchangeName",
        "accountName",
        "instanceId",
        "joinProof",
    ] {
        let mut changed = input.clone();
        changed.as_object_mut().unwrap().remove(field);
        assert!(
            account
                .prepare(&changed.to_string(), NOW, &[9; 32])
                .is_err(),
            "{field}"
        );
    }
    for invalid in [
        raw.replace("\"version\":1", "\"version\":1,\"version\":1"),
        raw.replace("\"backpack\"", "\"hyperliquid\""),
        raw.replace("\"join-proof\"", "{\"join-proof\":null}"),
        format!("{raw}garbage"),
        "{\"kind\":\"http\",\"payload\":\"withdraw\"}".into(),
        "[\"join-proof\",\"caller-digest\"]".into(),
    ] {
        assert!(account.prepare(&invalid, NOW, &[9; 32]).is_err());
    }
    assert!(
        account.prepare(&raw, NOW - 1, &[9; 32]).is_err(),
        "future challenge"
    );
    assert!(
        account.prepare(&raw, NOW + 15_001, &[9; 32]).is_err(),
        "expired challenge"
    );
}

#[test]
fn missing_owned_identity_and_hmac_venue_are_unavailable() {
    for venue in [
        "backpack",
        "hyperliquid",
        "paradex",
        "lighter",
        "bybit",
        "unknown",
    ] {
        assert!(Account::capture(venue, &Default::default(), &options()).is_err());
    }
}

#[test]
fn proof_results_cannot_export_headers_or_a_credential() {
    for extra in [false, true] {
        let (account, provider, _, raw, _) = fixture("backpack", Family::Ed25519);
        let (work, pending) = account.prepare(&raw, NOW, &[9; 32]).unwrap();
        let mut result: SignResult = provider.sign_ready(&work).unwrap().unwrap();
        if extra {
            result.values.push(b"must-not-export".to_vec());
        } else {
            result
                .headers
                .push(("X-Key".into(), "must-not-export".into()));
        }
        assert!(pending.finish(&result).is_err());
    }
}

#[test]
fn present_owned_refs_do_not_turn_an_hmac_venue_into_a_public_key_recipe() {
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("public".into()),
            Some(KeyRef("signing".into())),
        )),
        secondary: None,
    };
    for venue in ["bybit", "binance", "unknown"] {
        assert!(Account::capture(venue, &ids, &options()).is_err());
    }
}

#[cfg(feature = "asterdex")]
#[test]
fn asterdex_requires_explicit_v3_policy_and_keeps_v1_closed() {
    let ids = Identities {
        primary: Some(Identity::new(
            KeyRef("public".into()),
            Some(KeyRef("signing".into())),
        )),
        secondary: None,
    };
    assert!(Account::capture("asterdex", &ids, &options()).is_err());
    let v1 = serde_json::from_str(r#"{"asterdex":{"apiVersion":"v1"}}"#).unwrap();
    assert!(Account::capture("asterdex", &ids, &v1).is_err());
    let v3 = serde_json::from_str(r#"{"asterdex":{"apiVersion":"v3"}}"#).unwrap();
    let account = Account::capture("asterdex", &ids, &v3).unwrap();
    let statement = Statement::new("aster-proof", [1; 32], [2; 32], NOW).unwrap();
    let request = Request::new(
        "aster-proof",
        Venue::Asterdex,
        "account",
        "registration",
        statement,
    )
    .unwrap();
    let (work, _) = account
        .prepare(&serde_json::to_string(&request).unwrap(), NOW, &[9; 32])
        .unwrap();
    let Operation::Sign { algorithm, .. } = work.operations[0] else {
        panic!("expected signing")
    };
    assert_eq!(algorithm, Family::Ethereum.algorithm());
}
