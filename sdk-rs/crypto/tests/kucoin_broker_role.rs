//! NEW-CONTRACT broker specifications; schema rejection is distinct from native REDs.
use safe_cex_crypto::signer::LocalSigner;
use safe_cex_engine::signing::{
    Algorithm, Capability, CredentialSetupPolicy, ExtraKey, Identities, KeyRef, Operation,
    PayloadPart, SignError, SignRequest, Signer,
};

fn policy() -> CredentialSetupPolicy {
    serde_json::from_str(
        r#"{"primary":{"extra":{"key":"kucoinBroker","field":"optional","encoding":"utf8","trim":true}},"secondary":{"extra":{"key":"kucoinBroker","field":"aliasPrimary"}}}"#,
    )
    .expect("NEW-CONTRACT: declared broker policy must decode")
}
fn role() -> ExtraKey {
    serde_json::from_str(r#""kucoinBroker""#).expect("NEW-CONTRACT: broker role must decode")
}
fn setup(material: Option<&str>, secondary: bool, secondary_material: bool) -> String {
    let mut primary = serde_json::json!({"key":"api-key","secret":"api-secret"});
    if let Some(material) = material {
        primary["extra"] = serde_json::json!({"kucoinBroker":{"value":material}});
    }
    let mut value = serde_json::json!({"primary":primary});
    if secondary {
        value["secondary"] = serde_json::json!({"key":"read-key","secret":"read-secret"});
        if secondary_material {
            value["secondary"]["extra"] =
                serde_json::json!({"kucoinBroker":{"value":"other-broker"}});
        }
    }
    value.to_string()
}
fn install(material: Option<&str>, secondary: bool) -> (LocalSigner, Identities) {
    LocalSigner::from_json_with_policy(setup(material, secondary, false), policy()).unwrap()
}
fn operation(key: KeyRef, payload: Vec<PayloadPart>) -> SignRequest {
    SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key,
            payload,
        }],
    }
}
fn broker_mac(signer: &LocalSigner, broker: &KeyRef, api: &KeyRef) -> Result<Vec<u8>, SignError> {
    signer
        .sign_now(&operation(
            broker.clone(),
            vec![
                PayloadPart::Bytes {
                    bytes: b"1700000000000partner".to_vec(),
                },
                PayloadPart::Credential { key: api.clone() },
            ],
        ))
        .map(|result| result.values[0].clone())
}

#[test]
fn new_contract_broker_role_has_a_public_wire_name() {
    assert_eq!(role().name(), "kucoinBroker");
}

#[test]
fn new_contract_broker_signs_the_selected_key_against_an_independent_vector() {
    let (signer, ids) = install(Some("broker-secret"), false);
    let identity = ids.primary.unwrap();
    let broker = &identity.extra[&role()];
    let result = broker_mac(&signer, broker, &identity.credential).unwrap();
    assert_eq!(
        safe_cex_crypto::encode(&result, safe_cex_crypto::Encoding::Base64),
        "6TOQ13udAdzEs6zUIjQuyXJkT3x6HJA86DxAYCdT3O4="
    );
}

#[test]
fn new_contract_broker_trim_matches_rust_and_preserves_bom() {
    for (material, expected) in [
        ("  b \t", "6tFIL+IGtjW6hoFO7pS60ZmCM0YDllzXKJJD2gmIWkE="),
        ("\u{feff}b", "v1Oqtv46/eSZaz8cGYUJ/RVBBm7WVtMqX/gSndiA+rc="),
    ] {
        let (signer, ids) = install(Some(material), false);
        let identity = ids.primary.unwrap();
        let result = broker_mac(&signer, &identity.extra[&role()], &identity.credential).unwrap();
        assert_eq!(
            safe_cex_crypto::encode(&result, safe_cex_crypto::Encoding::Base64),
            expected
        );
    }
    for material in [None, Some(""), Some("  \t ")] {
        let (_, ids) = install(material, true);
        assert!(!ids.primary.unwrap().extra.contains_key(&role()));
        assert!(!ids.secondary.unwrap().extra.contains_key(&role()));
    }
}

#[test]
fn new_contract_secondary_alias_names_the_primary_ref_and_rejects_own_material() {
    let (_, ids) = install(Some("broker-secret"), true);
    assert_eq!(
        ids.primary.as_ref().unwrap().extra.get(&role()),
        ids.secondary.as_ref().unwrap().extra.get(&role())
    );
    assert!(ids.primary.as_ref().unwrap().extra.contains_key(&role()));
    assert!(
        LocalSigner::from_json_with_policy(setup(Some("broker-secret"), true, true), policy())
            .is_err()
    );
}

#[test]
fn new_contract_broker_ref_is_unpresentable_and_never_credential_data() {
    let (signer, ids) = install(Some("broker-secret"), false);
    let identity = ids.primary.unwrap();
    let broker = identity.extra[&role()].clone();
    let present = SignRequest {
        receipt: Default::default(),
        context: Default::default(),
        operations: vec![Operation::Present {
            key: broker.clone(),
        }],
    };
    assert_eq!(signer.sign_now(&present), Err(SignError::InvalidRequest));
    assert_eq!(
        signer.sign_now(&operation(
            identity.signing,
            vec![PayloadPart::Credential { key: broker }]
        )),
        Err(SignError::InvalidRequest)
    );
}

#[test]
fn new_contract_broker_role_refuses_every_other_algorithm() {
    let (signer, ids) = install(Some("broker-secret"), false);
    let broker = ids.primary.unwrap().extra[&role()].clone();
    assert_eq!(
        signer.can_sign(&broker, Algorithm::HmacSha256),
        Capability::Supported
    );
    let algorithms =
        std::iter::successors(Some(Algorithm::HmacSha256), |algorithm| match algorithm {
            Algorithm::HmacSha256 => Some(Algorithm::HmacSha512),
            Algorithm::HmacSha512 => Some(Algorithm::Sha256SecretSuffix),
            Algorithm::Sha256SecretSuffix => Some(Algorithm::Ed25519),
            Algorithm::Ed25519 => Some(Algorithm::Es256),
            Algorithm::Es256 => Some(Algorithm::Secp256k1Digest),
            Algorithm::Secp256k1Digest => Some(Algorithm::StarkDigest),
            Algorithm::StarkDigest => Some(Algorithm::SchnorrDigest),
            Algorithm::SchnorrDigest => Some(Algorithm::ZkLinkContract),
            Algorithm::ZkLinkContract => None,
        });
    for algorithm in algorithms.skip(1) {
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                key: broker.clone(),
                algorithm,
                payload: vec![PayloadPart::Bytes {
                    bytes: b"m".to_vec(),
                }],
            }],
        };
        assert_eq!(signer.sign_now(&request), Err(SignError::InvalidRequest));
        assert_eq!(signer.can_sign(&broker, algorithm), Capability::Unsupported);
    }
}

#[test]
fn existing_ordinary_hmac_provider_is_a_working_control() {
    let (signer, ids) =
        LocalSigner::from_json(r#"{"primary":{"key":"api-key","secret":"api-secret"}}"#.into())
            .unwrap();
    let request = operation(
        ids.primary.unwrap().signing,
        vec![PayloadPart::Bytes {
            bytes: b"m".to_vec(),
        }],
    );
    assert!(signer.sign_now(&request).is_ok());
}

#[test]
fn existing_role_ref_cannot_cross_into_hmac_or_presentation() {
    let policy: CredentialSetupPolicy = serde_json::from_str(
        r#"{"primary":{"extra":{"key":"zklinkSeed","field":"optional","encoding":"utf8"}}}"#,
    )
    .unwrap();
    let (signer, ids) = LocalSigner::from_json_with_policy(
        r#"{"primary":{"key":"api-key","secret":"api-secret","extra":{"zklinkSeed":{"value":"synthetic-seed"}}}}"#.into(), policy,
    ).unwrap();
    let key = ids.primary.unwrap().extra[&ExtraKey::ZklinkSeed].clone();
    assert_eq!(
        signer.sign_now(&operation(key.clone(), vec![])),
        Err(SignError::InvalidRequest)
    );
    assert_eq!(
        signer.can_sign(&key, Algorithm::HmacSha256),
        Capability::Unsupported
    );
    assert_eq!(
        signer.sign_now(&SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present { key }],
        }),
        Err(SignError::InvalidRequest)
    );
}

#[test]
fn new_contract_broker_alias_debug_contains_only_opaque_refs() {
    let (_, ids) = install(Some("broker-secret"), true);
    let debug = format!("{ids:?}");
    for material in [
        "api-key",
        "api-secret",
        "read-key",
        "read-secret",
        "broker-secret",
    ] {
        assert!(!debug.contains(material));
    }
    assert_eq!(
        ids.primary.as_ref().unwrap().extra.get(&role()),
        ids.secondary.as_ref().unwrap().extra.get(&role())
    );
}

#[test]
fn new_contract_secondary_without_primary_has_no_broker_alias() {
    let input = serde_json::json!({
        "secondary": {"key": "read-key", "secret": "read-secret"}
    });
    let (_, ids) = LocalSigner::from_json_with_policy(input.to_string(), policy())
        .expect("secondary-only setup succeeds without a primary broker to alias");
    assert!(ids.primary.is_none());
    assert!(!ids.secondary.unwrap().extra.contains_key(&role()));
}
