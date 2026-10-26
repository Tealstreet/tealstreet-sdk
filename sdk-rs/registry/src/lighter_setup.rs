//! Trusted Lighter setup keeps the original key facts and only public lookup metadata.
use crate::{EngineOptions, VenueError, invalid};
use safe_cex_crypto::signer::{LighterPrincipal, LocalSetup, LocalSigner};
use safe_cex_engine::signing::{CredentialSetupPolicy, Identities, Provider, SetupFacts};

fn public_address(key: &str) -> Option<String> {
    let start = key.find("0x")?;
    let address = key.get(start..start + 42)?;
    (address[2..].bytes().all(|b| b.is_ascii_hexdigit())
        && key
            .as_bytes()
            .get(start + 42)
            .is_none_or(|b| !b.is_ascii_hexdigit()))
    .then(|| address.to_owned())
}

pub(crate) fn capture_lookup(options: &mut EngineOptions) {
    if options.lighter_l1_address.is_none() {
        options.lighter_l1_address = options.key.as_deref().and_then(public_address);
    }
}

pub(crate) fn setup_public_mode(options: &EngineOptions) -> bool {
    options
        .public_mode
        .as_ref()
        .and_then(|mode| mode.requested)
        .unwrap_or(options.public_data_only)
}

fn install_with_facts(
    setup: LocalSetup,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities, SetupFacts), VenueError> {
    let missing_index = options.api_key_index.is_none();
    let public = crate::construction::signer_setup_public_data_only(options)
        || (missing_index && setup_public_mode(options));
    if !public
        && options
            .lighter_signer_provider
            .as_deref()
            .is_some_and(|provider| !provider.is_empty() && provider != "tealstreet-sdk")
    {
        return Err(invalid("Unsupported Lighter signer provider"));
    }
    let index = if public || missing_index || setup.primary.is_none() {
        0
    } else {
        match options
            .api_key_index
            .as_deref()
            .and_then(|index| index.parse::<u64>().ok())
        {
            Some(index) => index,
            None => return Err(invalid("Lighter requires an API key index in 0..=254")),
        }
    };
    let principal = LighterPrincipal::new(index)
        .map_err(|_| invalid("Lighter requires an API key index in 0..=254"))?;
    let (signer, identities, facts) = LocalSigner::from_setup_with_lighter_principal(
        setup,
        policy,
        public || missing_index,
        principal,
    )
    .map_err(|_| {
        invalid(if missing_index && !public {
            "Lighter requires an API key index in 0..=254"
        } else {
            "Invalid Lighter local signer setup"
        })
    })?;
    if missing_index {
        if !public
            && identities
                .primary
                .as_ref()
                .is_some_and(|primary| primary.signing_opt().is_some())
        {
            return Err(invalid("Lighter requires an API key index in 0..=254"));
        }
        // No API index grants no private authority; retain facts, wipe material.
        drop(signer);
        return Ok((
            std::sync::Arc::new(LocalSigner::new()),
            Identities::default(),
            facts,
        ));
    }
    Ok((std::sync::Arc::new(signer), identities, facts))
}

pub(crate) fn native(
    options: &mut EngineOptions,
    policy: CredentialSetupPolicy,
) -> Result<(Provider, Identities), VenueError> {
    use safe_cex_crypto::signer::{LocalIdentity, Secret};
    capture_lookup(options);
    if options.read_credentials.is_some() {
        return Err(invalid("Lighter has no secondary signing identity"));
    }
    let primary = if options.key.is_some() || options.secret.is_some() {
        let mut primary = LocalIdentity::default();
        if let Some(key) = options.key.take() {
            primary.key = Secret::new(key);
        }
        if let Some(secret) = options.secret.take() {
            primary.secret = Secret::new(secret);
        }
        Some(primary)
    } else {
        None
    };
    // Original facts are intentionally discarded: Lighter has no login intent.
    let (provider, identities, _original_facts) = install_with_facts(
        LocalSetup {
            primary,
            secondary: None,
        },
        policy,
        options,
    )?;
    Ok((provider, identities))
}

pub(crate) fn json(
    input: String,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let input = zeroize::Zeroizing::new(input);
    let setup = serde_json::from_str::<LocalSetup>(&input)
        .map_err(|_| invalid("Invalid Lighter local signer setup"))?;
    // Original facts are intentionally discarded: Lighter has no login intent.
    let (provider, identities, _original_facts) = install_with_facts(setup, policy, options)?;
    Ok((provider, identities))
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{
        Algorithm, Capability, KeyPresence, Operation, PayloadPart, SignError, SignRequest,
    };
    use serde_json::{Value, json};

    fn engine_options() -> EngineOptions {
        serde_json::from_value(json!({"apiKeyIndex":"1","applicationId":"718895"})).unwrap()
    }

    fn setup(key: Option<Value>) -> LocalSetup {
        let mut primary = json!({"secret":format!("01{}", "00".repeat(39))});
        if let Some(key) = key {
            primary["key"] = key;
        }
        serde_json::from_value(json!({"primary":primary})).unwrap()
    }

    fn present(
        provider: &Provider,
        key: safe_cex_engine::signing::KeyRef,
    ) -> Result<Vec<u8>, SignError> {
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present { key }],
        };
        provider
            .sign_ready(&request)
            .unwrap()
            .map(|result| result.values[0].clone())
    }

    fn assert_no_signing_material(provider: &Provider) {
        let key = safe_cex_engine::signing::KeyRef("primary.signing".into());
        assert_eq!(
            provider.can_sign(&key, Algorithm::SchnorrDigest),
            Capability::Unsupported
        );
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key,
                payload: vec![PayloadPart::Bytes {
                    bytes: [vec![7; 32], b"12345678".to_vec()].concat(),
                }],
            }],
        };
        assert!(provider.sign_ready(&request).unwrap().is_err());
    }

    #[test]
    fn original_key_facts_reach_registry_before_principal_mapping() {
        for (key, expected) in [
            (None, KeyPresence::Absent),
            (Some(Value::Null), KeyPresence::Absent),
            (Some(json!("")), KeyPresence::Empty),
            (Some(json!(" \t")), KeyPresence::Blank),
            (Some(json!("native-key")), KeyPresence::Nonblank),
        ] {
            let options = engine_options();
            let (provider, identities, facts) = install_with_facts(
                setup(key),
                crate::signer_setup_policy_for("lighter", &options),
                &options,
            )
            .unwrap();
            assert_eq!(facts.primary_key(), expected);
            let identity = identities.primary.unwrap();
            assert!(identity.signing_ref().is_ok());
            if expected == KeyPresence::Nonblank {
                assert_eq!(
                    present(&provider, identity.credential).unwrap(),
                    b"native-key"
                );
            } else {
                assert_eq!(identity.credential.0, "primary.principal");
                assert!(present(&provider, identity.credential).is_err());
            }
        }
    }

    #[test]
    fn native_and_separate_json_setup_issue_the_same_opaque_membership() {
        for venue in ["lighter", "lighter-robinhood"] {
            let mut native_options = engine_options();
            native_options.secret = Some(format!("01{}", "00".repeat(39)));
            native_options.key = Some(" \t".into());
            let policy = crate::signer_setup_policy_for(venue, &native_options);
            let (provider, native_ids) = native(&mut native_options, policy).unwrap();
            assert!(native_options.secret.is_none() && native_options.key.is_none());
            let (json_provider, json_ids) = json(
                json!({"primary":{"key":" \t","secret":format!("01{}", "00".repeat(39))}})
                    .to_string(),
                crate::signer_setup_policy_for(venue, &native_options),
                &native_options,
            )
            .unwrap();
            let native = native_ids.primary.unwrap();
            let separate = json_ids.primary.unwrap();
            assert_eq!(native.credential, separate.credential);
            assert_eq!(
                native.signing_ref().unwrap(),
                separate.signing_ref().unwrap()
            );
            assert!(present(&provider, native.credential).is_err());
            assert!(present(&json_provider, separate.credential).is_err());
        }
    }

    #[test]
    fn public_only_and_no_material_cannot_acquire_principal_membership() {
        let mut options = engine_options();
        options.public_data_only = true;
        let (_, identities, _) = install_with_facts(
            setup(None),
            crate::signer_setup_policy_for("lighter", &options),
            &options,
        )
        .unwrap();
        assert!(identities.primary.unwrap().credential.0.is_empty());
        options.public_data_only = false;
        let (_, identities) = json(
            "{}".into(),
            crate::signer_setup_policy_for("lighter", &options),
            &options,
        )
        .unwrap();
        assert!(identities.primary.is_none());
    }

    #[test]
    fn missing_api_index_refuses_a_private_scalar() {
        for explicit in [false, true] {
            let mut options = engine_options();
            options.api_key_index = None;
            if explicit {
                options.public_data_only = true;
                options.public_mode =
                    Some(serde_json::from_value(json!({"requested":false})).unwrap());
            }
            let error = install_with_facts(
                setup(None),
                crate::signer_setup_policy_for("lighter", &options),
                &options,
            )
            .err()
            .expect("a private scalar needs its API index");
            assert_eq!(
                error.message,
                "Lighter requires an API key index in 0..=254"
            );
        }
    }

    #[test]
    fn missing_api_index_with_empty_or_absent_material_stays_public() {
        for input in [json!({}), json!({"primary":{"key":"","secret":""}})] {
            let mut options = engine_options();
            options.api_key_index = None;
            let (provider, identities, facts) = install_with_facts(
                serde_json::from_value(input.clone()).unwrap(),
                crate::signer_setup_policy_for("lighter", &options),
                &options,
            )
            .unwrap();
            assert!(identities.primary.is_none());
            assert_no_signing_material(&provider);
            assert_eq!(
                facts.primary_key(),
                if input.get("primary").is_some() {
                    KeyPresence::Empty
                } else {
                    KeyPresence::Absent
                }
            );
            assert!(
                present(
                    &provider,
                    safe_cex_engine::signing::KeyRef("primary.signing".into())
                )
                .is_err()
            );
        }
    }

    #[test]
    fn inferred_public_startup_without_an_index_discards_its_scalar() {
        let mut options = engine_options();
        options.api_key_index = None;
        options.public_data_only = true;
        options.public_mode = Some(serde_json::from_value(json!({"requested":null})).unwrap());
        let (provider, identities, facts) = install_with_facts(
            setup(None),
            crate::signer_setup_policy_for("lighter", &options),
            &options,
        )
        .unwrap();
        assert!(identities.primary.is_none());
        assert_no_signing_material(&provider);
        assert_eq!(facts.primary_key(), KeyPresence::Absent);
        assert!(
            present(
                &provider,
                safe_cex_engine::signing::KeyRef("primary.signing".into())
            )
            .is_err()
        );
    }

    #[test]
    fn unsupported_payload_signer_precedes_local_scalar_validation() {
        for venue in ["lighter", "lighter-robinhood"] {
            let mut options = engine_options();
            options.payload_signer = true;
            options.secret = Some("invalid-scalar".into());
            let error = crate::prepare_local_signer_for(venue, &mut options)
                .err()
                .expect("unsupported payload signer");
            assert_eq!(error.kind, Some(crate::ErrorKind::Unsupported));
            options.secret = None;
            options.public_mode = Some(
                serde_json::from_value(json!({
                    "requested":null,"apiKeyIndexPresent":true
                }))
                .unwrap(),
            );
            let error =
                crate::local_signer_from_json_for_mode(venue, "invalid-json".into(), &options)
                    .err()
                    .expect("unsupported payload signer");
            assert_eq!(error.kind, Some(crate::ErrorKind::Unsupported));
            options.public_data_only = false;
            options.public_mode = Some(serde_json::from_value(json!({"requested":true})).unwrap());
            assert!(crate::local_signer_from_json_for_mode(venue, "{}".into(), &options).is_ok());
        }
    }

    #[test]
    fn invalid_scalar_index_secondary_and_provider_refuse_before_build() {
        for input in [
            json!({"primary":{"secret":"00".repeat(40)}}),
            json!({"primary":{"secret":"01"}}),
            json!({"primary":{"secret":format!("01{}", "00".repeat(39))},"secondary":{"key":"read"}}),
        ] {
            let options = engine_options();
            assert!(
                json(
                    input.to_string(),
                    crate::signer_setup_policy_for("lighter", &options),
                    &options
                )
                .is_err()
            );
        }
        for index in ["255", "-1", "invalid"] {
            let mut options = engine_options();
            options.api_key_index = Some(index.into());
            assert!(
                install_with_facts(
                    setup(None),
                    crate::signer_setup_policy_for("lighter", &options),
                    &options
                )
                .is_err()
            );
        }
        let mut options = engine_options();
        options.lighter_signer_provider = Some("unreviewed-provider".into());
        assert!(
            install_with_facts(
                setup(None),
                crate::signer_setup_policy_for("lighter", &options),
                &options
            )
            .is_err()
        );
    }

    #[test]
    fn public_lookup_retains_only_a_complete_address_before_stripping() {
        let address = format!("0x{}", "ab".repeat(20));
        for key in [address.clone(), format!("L1 account: {address}")] {
            let mut options = engine_options();
            options.key = Some(key);
            capture_lookup(&mut options);
            assert_eq!(
                options.lighter_l1_address.as_deref(),
                Some(address.as_str())
            );
        }
        for key in [
            format!("{address}abcd"),
            "not an address".into(),
            "0x01".into(),
        ] {
            let mut options = engine_options();
            options.key = Some(key);
            capture_lookup(&mut options);
            assert!(options.lighter_l1_address.is_none());
        }
    }

    #[test]
    fn credential_only_host_injection_is_still_refused() {
        assert!(
            serde_json::from_value::<EngineOptions>(json!({
                "signing":{"primary":{"credential":"host.principal"}},
                "apiKeyIndex":"1","applicationId":"718895"
            }))
            .is_err()
        );
    }

    struct Installed;
    impl crate::Visit for Installed {
        type Output = (bool, bool);
        fn visit<V: safe_cex_engine::Venue + 'static>(
            self,
            engine: safe_cex_engine::Engine<V>,
        ) -> Self::Output {
            (
                engine
                    .identities()
                    .primary
                    .as_ref()
                    .is_some_and(|id| id.signing_ref().is_ok()),
                engine.signer().is_some(),
            )
        }
    }

    #[test]
    fn registry_constructs_from_issued_refs_and_refuses_material_or_false_public_metadata() {
        for venue in ["lighter", "lighter-robinhood"] {
            let mut options = engine_options();
            options.secret = Some(format!("01{}", "00".repeat(39)));
            let policy = crate::signer_setup_policy_for(venue, &options);
            let (provider, identities) = native(&mut options, policy).unwrap();
            options.signing = Some(identities);
            assert_eq!(
                crate::build_with_signer(
                    venue,
                    options,
                    Some(safe_cex_engine::test_support::seeded_entropy(17)),
                    Some(provider),
                    Installed
                )
                .unwrap(),
                (true, true)
            );
            let mut raw = engine_options();
            raw.secret = Some("raw-material".into());
            assert!(
                crate::build_with_signer(
                    venue,
                    raw,
                    Some(safe_cex_engine::test_support::seeded_entropy(17)),
                    None,
                    Installed
                )
                .is_err()
            );
            let mut metadata = engine_options();
            metadata.lighter_l1_address = Some("private-material-is-not-an-address".into());
            assert!(
                crate::build_with_signer(
                    venue,
                    metadata,
                    Some(safe_cex_engine::test_support::seeded_entropy(17)),
                    None,
                    Installed
                )
                .is_err()
            );
        }
    }
}
