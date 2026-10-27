//! Trusted local agent setup. Account addresses stay public; only the provider owns the scalar.
use crate::{EngineOptions, VenueError, invalid};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{CredentialSetupPolicy, Identities, KeyPresence, Provider};

const INVALID: &str = "Invalid Hyperliquid local signer setup";

fn install(
    setup: LocalSetup,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    if setup.secondary.is_some()
        || setup
            .primary
            .as_ref()
            .is_some_and(|primary| primary.passphrase.is_some())
    {
        return Err(invalid(INVALID));
    }
    let (signer, identities, facts) = LocalSigner::from_setup_with_facts(
        setup,
        policy,
        crate::signer_setup_public_data_only(options),
    )
    .map_err(|_| invalid(INVALID))?;
    // A public account address is never a supplied provider credential.
    if facts.primary_key() != KeyPresence::Absent {
        return Err(invalid(INVALID));
    }
    Ok((std::sync::Arc::new(signer), identities))
}

pub(crate) fn native(
    options: &mut EngineOptions,
    policy: CredentialSetupPolicy,
) -> Result<(Provider, Identities), VenueError> {
    drop(options.passphrase.take().map(Secret::new));
    let secret = options.secret.take().map(Secret::new);
    if options.read_credentials.is_some() {
        return Err(invalid(INVALID));
    }
    let primary = secret.map(|secret| LocalIdentity {
        secret,
        ..Default::default()
    });
    install(
        LocalSetup {
            primary,
            secondary: None,
        },
        policy,
        options,
    )
}

pub(crate) fn json(
    input: String,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let input = zeroize::Zeroizing::new(input);
    let setup = serde_json::from_str::<LocalSetup>(&input).map_err(|_| invalid(INVALID))?;
    install(setup, policy, options)
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{Operation, SignRequest};
    use serde_json::json;

    fn options(secret: &str) -> EngineOptions {
        serde_json::from_value(json!({
            "key":"0x1111111111111111111111111111111111111111",
            "applicationId":"0x2222222222222222222222222222222222222222",
            "vault":"0x3333333333333333333333333333333333333333",
            "secret":secret
        }))
        .unwrap()
    }

    fn address(provider: &Provider, identities: &Identities) -> Option<Vec<u8>> {
        let identity = identities.primary.as_ref()?;
        assert!(identity.credential_absent());
        assert!(identity.credential_ref().is_none());
        assert!(identity.stable_id.is_none());
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::PublicAddress {
                key: identity.signing_ref().unwrap().clone(),
            }],
        };
        provider.sign_ready(&request)?.ok()?.values.first().cloned()
    }

    #[test]
    fn native_and_private_json_share_the_exact_scalar_decoder() {
        #[derive(serde::Deserialize)]
        struct ScalarRow {
            name: String,
            scalar: String,
            valid: bool,
        }
        let rows: Vec<ScalarRow> = serde_json::from_str(include_str!(
            "../../../packages/safe-cex/src/exchanges/rust/hyperliquid-signer-setup.golden.json"
        ))
        .unwrap();
        let mut expected = None;
        for venue in ["hyperliquid", "hyperliquidspot"] {
            for row in &rows {
                let scalar = &row.scalar;
                let mut options = options(scalar);
                let key = options.key.clone();
                let user = options.application_id.clone();
                let vault = options.vault.clone();
                let native = crate::prepare_local_signer_for(venue, &mut options);
                assert!(options.secret.is_none());
                assert_eq!(options.key, key);
                assert_eq!(options.application_id, user);
                assert_eq!(options.vault, vault);
                let separate = crate::local_signer_from_json_for_mode(
                    venue,
                    json!({"primary":{"secret":scalar,"secretEncoding":"utf8"}}).to_string(),
                    &options,
                );
                let result = |setup: Result<(Provider, Identities), VenueError>| match setup {
                    Ok((provider, identities)) => address(&provider, &identities),
                    Err(error) => {
                        assert_eq!(error.message, INVALID);
                        assert!(!format!("{error:?}").contains(scalar));
                        None
                    }
                };
                let native = result(native);
                assert_eq!(native.is_some(), row.valid, "{}", row.name);
                assert_eq!(result(separate), native, "{}", row.name);
                if let Some(native) = native {
                    assert_eq!(*expected.get_or_insert_with(|| native.clone()), native);
                }
            }
        }
    }

    #[test]
    fn legacy_top_level_passphrase_is_discarded_before_provider_setup() {
        let scalar = format!("{}ab", "00".repeat(31));
        for venue in ["hyperliquid", "hyperliquidspot"] {
            let mut expected = None;
            for passphrase in [None, Some(""), Some("ignored-passphrase-sentinel")] {
                let mut options = options(&scalar);
                options.passphrase = passphrase.map(str::to_owned);
                let (provider, identities) =
                    crate::prepare_local_signer_for(venue, &mut options).unwrap();
                assert!(options.passphrase.is_none());
                assert!(options.secret.is_none());
                let actual = address(&provider, &identities).unwrap();
                assert_eq!(*expected.get_or_insert_with(|| actual.clone()), actual);
                let identity = identities.primary.unwrap();
                assert!(identity.passphrase.is_none());
                assert!(!format!("{identity:?}").contains("ignored-passphrase-sentinel"));
            }
            let mut invalid_options = options("invalid-scalar");
            invalid_options.passphrase = Some("ignored-passphrase-sentinel".into());
            let error = crate::prepare_local_signer_for(venue, &mut invalid_options)
                .err()
                .unwrap();
            assert!(invalid_options.passphrase.is_none());
            assert_eq!(error.message, INVALID);
            assert!(!format!("{error:?}").contains("ignored-passphrase-sentinel"));
        }
    }

    #[test]
    fn supplied_provider_key_and_secondary_are_not_a_scalar_only_backdoor() {
        let options = options("unused");
        for input in [
            json!({"primary":{"key":"account","secret":"ab"}}),
            json!({"primary":{"key":"","secret":"ab"}}),
            json!({"primary":{"key":" ","secret":"ab"}}),
            json!({"secondary":{"key":"read","secret":"ab"}}),
            json!({"primary":{"secret":format!("{}ab", "00".repeat(31)),"passphrase":"ignored"}}),
            json!({"secondary":{"passphrase":"ignored"}}),
        ] {
            let error =
                crate::local_signer_from_json_for_mode("hyperliquid", input.to_string(), &options)
                    .err()
                    .expect("refuse supplied credential");
            assert_eq!(error.message, INVALID);
        }
    }

    #[test]
    fn scalar_only_policy_does_not_relabel_a_nonempty_credential() {
        let policy = crate::signer_setup_policy("hyperliquid");
        let setup = serde_json::from_value(json!({"primary":{
            "key":"supplied", "secret":format!("{}ab", "00".repeat(31))
        }}))
        .unwrap();
        let (_, identities, _) = LocalSigner::from_setup_with_facts(setup, policy, false).unwrap();
        assert!(!identities.primary.unwrap().credential_absent());
    }

    #[test]
    fn scalar_authority_cannot_be_presented_as_credential_material() {
        let mut options = options(&format!("{}ab", "00".repeat(31)));
        let (provider, identities) =
            crate::prepare_local_signer_for("hyperliquid", &mut options).unwrap();
        let identity = identities.primary.unwrap();
        let key = identity.signing_ref().unwrap().clone();
        let request = SignRequest {
            receipt: Default::default(),
            context: Default::default(),
            operations: vec![Operation::Present { key }],
        };
        assert!(provider.sign_ready(&request).unwrap().is_err());
    }
}
