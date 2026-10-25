//! WEEX trusted setup policy and injected-provider construction.
#![cfg(all(feature = "weex", feature = "dispatch-signing"))]

use safe_cex_engine::signing::{
    Algorithm, Context, Operation, PayloadPart, SignReceipt, SignRequest,
};
use safe_cex_engine::{Engine, ErrorKind, Venue};
use safe_cex_venues::{EngineOptions, Visit, build_with_signer, local_signer_from_json_for_mode};

/// Account options carrying only the public/private setup intent.
fn mode(public: bool) -> EngineOptions {
    serde_json::from_value(serde_json::json!({"publicDataOnly": public})).unwrap()
}

#[test]
fn complete_private_primary_requires_passphrase_in_trusted_setup() {
    let result = local_signer_from_json_for_mode(
        "weex",
        r#"{"primary":{"key":"k","secret":"s"}}"#.into(),
        &mode(false),
    );
    assert!(
        result.is_err(),
        "WEEX private setup requires its passphrase"
    );
}
#[test]
fn secondary_absent_or_null_passphrase_inherits_only_the_primary_ref() {
    for secondary in [
        r#"{"key":"r","secret":"t"}"#,
        r#"{"key":"r","secret":"t","passphrase":null}"#,
    ] {
        let setup = format!(
            r#"{{"primary":{{"key":"k","secret":"s","passphrase":"p","stableId":"account-primary"}},"secondary":{secondary}}}"#
        );
        let (_, identities) = local_signer_from_json_for_mode("weex", setup, &mode(false)).unwrap();
        let primary = identities.primary.unwrap();
        let secondary = identities.secondary.unwrap();
        assert!(primary.passphrase.is_some());
        assert_eq!(secondary.passphrase, primary.passphrase);
    }
}
#[test]
fn explicit_empty_secondary_never_falls_back_even_in_public_mode() {
    for public in [false, true] {
        let result = local_signer_from_json_for_mode(
            "weex",
            r#"{"primary":{"key":"k","secret":"s","passphrase":"p"},"secondary":{"key":"r","secret":"t","passphrase":""}}"#.into(), &mode(public));
        assert!(
            result.is_err(),
            "explicit empty cannot become primary fallback"
        );
    }
}
#[test]
fn public_bypasses_only_primary_passphrase_requirement() {
    assert!(
        local_signer_from_json_for_mode(
            "weex",
            r#"{"primary":{"key":"k","secret":"s"}}"#.into(),
            &mode(true)
        )
        .is_ok()
    );
    assert!(
        local_signer_from_json_for_mode(
            "weex",
            r#"{"primary":{"key":"k","secret":"s"},"secondary":{"key":"r","secret":"t"}}"#.into(),
            &mode(true)
        )
        .is_err()
    );
}
#[test]
fn passphrase_only_primary_can_supply_a_complete_secondary() {
    let (provider, identities) = local_signer_from_json_for_mode(
        "weex",
        r#"{"primary":{"passphrase":" p "},"secondary":{"key":"r","secret":"4142","stableId":"account-read"}}"#.into(), &mode(true)).unwrap();
    let primary = identities.primary.unwrap();
    let secondary = identities.secondary.unwrap();
    assert_eq!(secondary.passphrase, primary.passphrase);
    assert_eq!(secondary.require_stable_id(), Ok("account-read"));
    let request = SignRequest {
        receipt: SignReceipt::default(),
        context: Context::default(),
        operations: vec![
            Operation::Present {
                key: secondary.passphrase.clone().expect("fallback ref"),
            },
            Operation::Sign {
                algorithm: Algorithm::HmacSha256,
                key: secondary.signing_ref().unwrap().clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: b"message".to_vec(),
                }],
            },
        ],
    };
    let result = provider.sign_ready(&request).unwrap().unwrap();
    assert_eq!(result.values[0], b" p ");
    // Independent Python stdlib oracle: key is literal UTF-8 `4142`, not hex AB.
    assert_eq!(
        safe_cex_crypto::hex(&result.values[1]),
        "bc8e07959e47b5363e59ee12868d3b7e3e14c4b786d737d936ebca5466e41fde"
    );
}
#[test]
fn credential_passphrase_and_stable_ids_remain_exact_independent_metadata() {
    let (provider, identities) = local_signer_from_json_for_mode(
        "weex",
        r#"{"primary":{"key":" k ","secret":"s","passphrase":" p ","stableId":"account-primary"},"secondary":{"key":" r ","secret":"t","passphrase":" q ","stableId":"account-read"}}"#.into(), &mode(false)).unwrap();
    for (identity, stable, key, passphrase) in [
        (
            identities.primary.unwrap(),
            "account-primary",
            b" k ".as_slice(),
            b" p ".as_slice(),
        ),
        (
            identities.secondary.unwrap(),
            "account-read",
            b" r ".as_slice(),
            b" q ".as_slice(),
        ),
    ] {
        assert_eq!(identity.require_stable_id(), Ok(stable));
        let request = SignRequest {
            receipt: SignReceipt::default(),
            context: Context::default(),
            operations: vec![
                Operation::Present {
                    key: identity.credential,
                },
                Operation::Present {
                    key: identity.passphrase.unwrap(),
                },
            ],
        };
        let result = provider.sign_ready(&request).unwrap().unwrap();
        assert_eq!(result.values, vec![key.to_vec(), passphrase.to_vec()]);
    }
}
struct Admitted;
impl Visit for Admitted {
    type Output = ();
    fn visit<V: Venue + 'static>(self, _: Engine<V>) {}
}
#[test]
fn trusted_setup_constructs_weex_with_the_injected_provider() {
    let (provider, identities) = local_signer_from_json_for_mode(
        "weex",
        r#"{"primary":{"key":"k","secret":"s","passphrase":"p","stableId":"account-primary"}}"#
            .into(),
        &mode(false),
    )
    .unwrap();
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities.clone());
    build_with_signer(
        "weex",
        options,
        Some(safe_cex_engine::test_support::seeded_entropy(17)),
        Some(provider),
        Admitted,
    )
    .unwrap();
    // Opaque identities without a provider cannot sign anything.
    let mut options: EngineOptions = serde_json::from_str("{}").unwrap();
    options.signing = Some(identities);
    let error = build_with_signer("weex", options, None, None, Admitted).unwrap_err();
    assert_eq!(error.kind, Some(ErrorKind::InvalidInput));
}
#[test]
fn raw_credentials_beside_injected_identities_are_refused() {
    let (provider, identities) = local_signer_from_json_for_mode(
        "weex",
        r#"{"primary":{"key":"k","secret":"s","passphrase":"p"}}"#.into(),
        &mode(false),
    )
    .unwrap();
    let mut options: EngineOptions = serde_json::from_str(r#"{"key":"k"}"#).unwrap();
    options.signing = Some(identities);
    let error = build_with_signer("weex", options, None, Some(provider), Admitted).unwrap_err();
    assert_eq!(
        error.message,
        "WEEX credentials belong in signer setup, not engine options"
    );
}
