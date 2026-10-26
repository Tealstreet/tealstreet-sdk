use super::*;
use safe_cex_engine::signing::{Context, IdentitySetupPolicy, SignReceipt};
use serde_json::{Value, json};

fn policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            secret_encoding: Some(KeyEncoding::Hex),
            signing_algorithm: Some(Algorithm::SchnorrDigest),
            ..Default::default()
        }),
        ..Default::default()
    }
}

fn setup(key: Option<Value>, secret: bool) -> LocalSetup {
    let mut primary = json!({"stableId":"caller-account"});
    if let Some(key) = key {
        primary["key"] = key;
    }
    if secret {
        primary["secret"] = json!(format!("01{}", "00".repeat(39)));
    }
    serde_json::from_value(json!({"primary":primary})).unwrap()
}

fn request(operation: Operation) -> SignRequest {
    SignRequest {
        receipt: SignReceipt::default(),
        context: Context::default(),
        operations: vec![operation],
    }
}

#[cfg(feature = "schnorr")]
fn installed(
    key: Option<Value>,
    public_only: bool,
    index: u64,
) -> (LocalSigner, Identities, SetupFacts) {
    LocalSigner::from_setup_with_lighter_principal(
        setup(key, true),
        policy(),
        public_only,
        LighterPrincipal::new(index).unwrap(),
    )
    .expect("trusted Lighter setup remains available")
}

#[test]
fn lighter_principal_index_is_finite_native_public_metadata() {
    assert_eq!(LighterPrincipal::new(0).unwrap().api_key_index(), 0);
    assert_eq!(LighterPrincipal::new(254).unwrap().api_key_index(), 254);
    assert_eq!(LighterPrincipal::new(255), Err(SignError::InvalidRequest));
    assert_eq!(
        LighterPrincipal::new(u64::MAX),
        Err(SignError::InvalidRequest)
    );
}

#[test]
fn lighter_principal_cannot_be_injected_as_json_text() {
    assert!(serde_json::from_str::<LocalSetup>(r#"{"primary":{"principal":"anything"}}"#).is_err());
}

#[test]
fn ordinary_secret_only_setup_keeps_its_empty_credential() {
    let (signer, ids, facts) =
        LocalSigner::from_setup_with_facts(setup(None, true), policy(), false).unwrap();
    let primary = ids.primary.unwrap();
    assert_eq!(facts.primary_key(), KeyPresence::Absent);
    assert!(primary.credential.0.is_empty());
    assert!(primary.signing_ref().is_ok());
    assert!(
        signer
            .sign_now(&request(Operation::Present {
                key: primary.credential
            }))
            .is_err()
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_enables_secret_only_required_membership() {
    let (signer, ids, facts) = installed(None, false, 1);
    let primary = ids.primary.unwrap();
    assert_eq!(primary.validate(), Ok(()));
    assert_eq!(facts.primary_key(), KeyPresence::Absent);
    assert_ne!(&primary.credential, primary.signing_ref().unwrap());
    assert_eq!(primary.stable_id.as_deref(), Some("caller-account"));
    assert_eq!(
        signer.lighter_principal.as_ref().unwrap().1.api_key_index(),
        1
    );
    assert!(!signer.keys.contains_key(&primary.credential));
    assert_eq!(
        signer.can_sign(primary.signing_ref().unwrap(), Algorithm::SchnorrDigest),
        Capability::Unsupported
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_preserves_every_original_key_presence() {
    for (key, presence) in [
        (None, KeyPresence::Absent),
        (Some(Value::Null), KeyPresence::Absent),
        (Some(json!("")), KeyPresence::Empty),
        (Some(json!(" \t")), KeyPresence::Blank),
    ] {
        let (signer, ids, facts) = installed(key, false, 1);
        assert_eq!(facts.primary_key(), presence);
        assert!(ids.primary.unwrap().validate().is_ok());
        assert!(signer.lighter_principal.is_some());
    }
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_nonblank_key_keeps_present_and_precedence() {
    let (signer, ids, facts) = installed(Some(json!("supplied-public-key")), false, 1);
    let primary = ids.primary.unwrap();
    assert_eq!(facts.primary_key(), KeyPresence::Nonblank);
    assert!(signer.lighter_principal.is_none());
    assert_eq!(
        signer
            .sign_now(&request(Operation::Present {
                key: primary.credential
            }))
            .unwrap()
            .values,
        vec![b"supplied-public-key".to_vec()]
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_ref_is_fixed_and_provider_state_is_distinct() {
    let (a, a_ids, _) = installed(None, false, 1);
    let (b, b_ids, _) = installed(None, false, 254);
    assert_eq!(
        a_ids.primary.unwrap().credential,
        b_ids.primary.unwrap().credential
    );
    assert_ne!(
        a.lighter_principal.unwrap().1,
        b.lighter_principal.unwrap().1
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_never_presents_or_expands_into_payload() {
    let (signer, ids, _) = installed(None, false, 1);
    let key = ids.primary.unwrap().credential;
    assert!(
        signer
            .sign_now(&request(Operation::Present { key: key.clone() }))
            .is_err()
    );
    assert!(
        signer
            .sign_now(&request(Operation::Sha256 {
                payload: vec![PayloadPart::Credential { key }],
            }))
            .is_err()
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_never_signs_or_proves_an_address_or_public_key() {
    let (signer, ids, _) = installed(None, false, 1);
    let key = ids.primary.unwrap().credential;
    for op in [
        Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: key.clone(),
            payload: vec![PayloadPart::Bytes { bytes: vec![1] }],
        },
        Operation::Sign {
            algorithm: Algorithm::SchnorrDigest,
            key: key.clone(),
            payload: vec![PayloadPart::Bytes { bytes: vec![1; 40] }],
        },
        Operation::PublicAddress { key: key.clone() },
        Operation::PublicKey {
            algorithm: Algorithm::SchnorrDigest,
            key,
        },
    ] {
        assert!(signer.sign_now(&request(op)).is_err());
    }
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_resists_trusted_insert_and_remove_revokes_it() {
    let (mut signer, ids, _) = installed(None, false, 1);
    let key = ids.primary.unwrap().credential;
    signer.insert(key.clone(), b"hostile-public-bytes".to_vec());
    assert!(
        signer
            .sign_now(&request(Operation::Present { key: key.clone() }))
            .is_err()
    );
    assert!(
        signer
            .sign_now(&request(Operation::Sha256 {
                payload: vec![PayloadPart::Credential { key: key.clone() }]
            }))
            .is_err()
    );
    assert!(
        signer
            .insert_declared(key.clone(), vec![1; 40], Algorithm::SchnorrDigest)
            .is_err()
    );
    signer.remove(&key);
    assert!(signer.lighter_principal.is_none());
    assert!(
        signer
            .sign_now(&request(Operation::Present { key }))
            .is_err()
    );
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_does_not_replace_the_signing_ref_or_vector() {
    let (signer, ids, _) = installed(None, false, 1);
    let key = ids.primary.unwrap().signing_ref().unwrap().clone();
    let payload = [vec![7; 32], b"12345678".to_vec()].concat();
    let result = signer
        .sign_now(&request(Operation::Sign {
            algorithm: Algorithm::SchnorrDigest,
            key,
            payload: vec![PayloadPart::Bytes { bytes: payload }],
        }))
        .unwrap();
    let mut scalar = [0; 40];
    scalar[0] = 1;
    let nonce = crate::lighter::hedged_nonce(b"12345678", &scalar, &[7; 32]).unwrap();
    let expected = crate::lighter::schnorr_sign(b"12345678", &scalar, nonce).unwrap();
    assert_eq!(result.values, vec![expected.to_vec()]);
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_public_only_and_no_private_material_never_get_a_principal() {
    let (signer, ids, facts) = installed(None, true, 1);
    assert!(signer.lighter_principal.is_none());
    assert_eq!(facts.primary_key(), KeyPresence::Absent);
    assert!(ids.primary.unwrap().credential.0.is_empty());
    let (signer, ids, _) = LocalSigner::from_setup_with_lighter_principal(
        setup(None, false),
        policy(),
        false,
        LighterPrincipal::new(1).unwrap(),
    )
    .unwrap();
    assert!(signer.lighter_principal.is_none());
    assert!(ids.primary.unwrap().signing_ref().is_err());
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_requires_exact_declaration_and_no_secondary() {
    assert!(matches!(
        LocalSigner::from_setup_with_lighter_principal(
            setup(None, true),
            CredentialSetupPolicy::default(),
            false,
            LighterPrincipal::new(1).unwrap(),
        ),
        Err(SignError::InvalidRequest)
    ));
    let mut input = setup(None, true);
    input.secondary = Some(LocalIdentity::default());
    assert!(matches!(
        LocalSigner::from_setup_with_lighter_principal(
            input,
            policy(),
            false,
            LighterPrincipal::new(1).unwrap(),
        ),
        Err(SignError::InvalidRequest)
    ));
}

#[cfg(not(feature = "schnorr"))]
#[test]
fn lighter_principal_without_schnorr_is_unsupported() {
    assert!(matches!(
        LocalSigner::from_setup_with_lighter_principal(
            setup(None, true),
            policy(),
            false,
            LighterPrincipal::new(1).unwrap(),
        ),
        Err(SignError::Unsupported)
    ));
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_preserves_absent_and_supplied_stable_ids() {
    for stable_id in [None, Some("caller-picked-account".to_owned())] {
        let mut input = setup(None, true);
        input.primary.as_mut().unwrap().stable_id = stable_id.clone();
        let (_, ids, _) = LocalSigner::from_setup_with_lighter_principal(
            input,
            policy(),
            false,
            LighterPrincipal::new(1).unwrap(),
        )
        .unwrap();
        assert_eq!(ids.primary.unwrap().stable_id, stable_id);
    }
}

#[cfg(feature = "schnorr")]
#[test]
fn lighter_principal_refuses_invalid_scalar_before_installation() {
    for scalar in ["00".repeat(40), "ff".repeat(40), "01".repeat(39)] {
        let mut input = setup(None, true);
        input.primary.as_mut().unwrap().secret = Secret::new(scalar);
        assert!(matches!(
            LocalSigner::from_setup_with_lighter_principal(
                input,
                policy(),
                false,
                LighterPrincipal::new(1).unwrap(),
            ),
            Err(SignError::InvalidRequest)
        ));
    }
}
