//! Trusted local Stark setup. The account and subkey addresses stay public;
//! only the provider owns the scalar, and only setup ever sees the key text.
use crate::{EngineOptions, VenueError, invalid};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_crypto::starknet;
use safe_cex_engine::signing::{
    Algorithm, CredentialSetupPolicy, Identities, KeyPresence, Provider, Signer,
};
use zeroize::Zeroizing;

const INVALID: &str = "Invalid Paradex local signer setup";

// Same raw-intent rule as lighter_setup::setup_public_mode, before provider installation.
fn setup_public_mode(options: &EngineOptions) -> bool {
    options
        .public_mode
        .as_ref()
        .and_then(|mode| mode.requested)
        .unwrap_or(options.public_data_only)
}

/// The SDK's private capsule: the account's key text, nothing else.
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct Capsule {
    primary: Option<CapsuleKey>,
}
#[derive(serde::Deserialize)]
#[serde(deny_unknown_fields)]
struct CapsuleKey {
    #[serde(deserialize_with = "zeroizing")]
    secret: Zeroizing<String>,
}
fn zeroizing<'de, D: serde::Deserializer<'de>>(input: D) -> Result<Zeroizing<String>, D::Error> {
    <String as serde::Deserialize>::deserialize(input).map(Zeroizing::new)
}

/// A 66-character key is a Stark scalar, exactly as the venue always read it;
/// any other is an Ethereum key the Stark key derives from on this network.
fn install(
    key: Option<Zeroizing<String>>,
    policy: CredentialSetupPolicy,
    options: &mut EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    // Explicit private intent without both public addresses keeps the
    // registry's explicit-private refusal; the key is never parsed for it.
    let addressed = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    let eligible = !setup_public_mode(options)
        && addressed(&options.paradex_address)
        && addressed(&options.subkey_address);
    let key = key.filter(|key| !key.is_empty() && eligible);
    let ethereum = key.as_ref().is_some_and(|key| key.len() != 66);
    let scalar = key
        .map(|key| {
            if ethereum {
                let chain = safe_cex_venue_paradex::sign::ethereum_chain(options.testnet);
                starknet::derive_from_ethereum(&key, chain)
            } else {
                starknet::parse_stark_key(&key)
            }
        })
        .transpose()
        .map_err(|error| invalid(&error))?;
    let primary = scalar.as_ref().map(|scalar| LocalIdentity {
        secret: Secret::new(safe_cex_crypto::hex(&scalar[..])),
        ..Default::default()
    });
    let (signer, identities, facts) = LocalSigner::from_setup_with_facts(
        LocalSetup {
            primary,
            secondary: None,
        },
        policy,
        crate::signer_setup_public_data_only(options),
    )
    .map_err(|_| invalid(INVALID))?;
    if facts.primary_key() != KeyPresence::Absent {
        return Err(invalid(INVALID));
    }
    // The provider's sealed answer must be the key this setup computed itself.
    if let (Some(scalar), Some(identity)) = (scalar.as_ref(), identities.primary.as_ref()) {
        let expected = starknet::public_key_bytes(scalar);
        if identity.public_key_sealed_by(Algorithm::StarkDigest, signer.incarnation())
            != Ok(expected)
        {
            return Err(invalid(INVALID));
        }
    }
    options.paradex_ethereum_key = ethereum && identities.primary.is_some();
    Ok((std::sync::Arc::new(signer), identities))
}

fn refusals(venue: &str, options: &mut EngineOptions) -> Result<(), VenueError> {
    let public = setup_public_mode(options);
    let payload_rule = crate::payload_signing_rules_in_mode(venue, options, false, public);
    drop(options.passphrase.take().map(Secret::new));
    drop(options.secret.take().map(Secret::new));
    payload_rule?;
    if options.read_credentials.is_some() {
        return Err(invalid(INVALID));
    }
    Ok(())
}

pub(crate) fn native(
    venue: &str,
    options: &mut EngineOptions,
    policy: CredentialSetupPolicy,
) -> Result<(Provider, Identities), VenueError> {
    let key = options.key.take().map(Zeroizing::new);
    refusals(venue, options)?;
    install(key, policy, options)
}

pub(crate) fn json(
    venue: &str,
    input: String,
    policy: CredentialSetupPolicy,
    options: &mut EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let input = Zeroizing::new(input);
    // The capsule is the only key source on this path.
    let duplicate = options.key.take().map(Zeroizing::new).is_some();
    refusals(venue, options)?;
    if duplicate {
        return Err(invalid(INVALID));
    }
    let capsule = serde_json::from_str::<Capsule>(&input).map_err(|_| invalid(INVALID))?;
    install(
        capsule.primary.map(|primary| primary.secret),
        policy,
        options,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const STARK: &str = "0x0000000000000000000000000000000000000000000000000000000000000001";
    const ETHEREUM: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    fn options(key: Option<&str>, testnet: bool) -> EngineOptions {
        serde_json::from_value(json!({
            "testnet": testnet,
            "key": key,
            "paradexAddress": "0x0123",
            "subkeyAddress": "0x456",
        }))
        .unwrap()
    }

    /// The sealed public key under its own provider, and the recorded key form.
    fn sealed(provider: &Provider, identities: &Identities) -> [u8; 32] {
        identities
            .primary
            .as_ref()
            .unwrap()
            .public_key_sealed_by(Algorithm::StarkDigest, provider.incarnation())
            .unwrap()
    }

    fn native(key: &str, testnet: bool) -> Result<([u8; 32], bool), VenueError> {
        let mut options = options(Some(key), testnet);
        let (provider, identities) = crate::prepare_local_signer_for("paradex", &mut options)?;
        assert!(options.key.is_none());
        Ok((sealed(&provider, &identities), options.paradex_ethereum_key))
    }

    fn capsule(key: &str, testnet: bool) -> Result<([u8; 32], bool), VenueError> {
        let mut options = options(None, testnet);
        let (provider, identities) = crate::local_signer_from_capsule(
            "paradex",
            json!({"primary": {"secret": key}}).to_string(),
            &mut options,
        )?;
        Ok((sealed(&provider, &identities), options.paradex_ethereum_key))
    }

    #[test]
    fn both_key_forms_seal_the_same_key_natively_and_from_the_capsule() {
        for testnet in [false, true] {
            let stark = native(STARK, testnet).unwrap();
            assert_eq!(stark, capsule(STARK, testnet).unwrap());
            assert_eq!(
                stark,
                (
                    starknet::public_key_bytes(&{
                        let mut one = [0; 32];
                        one[31] = 1;
                        one
                    }),
                    false
                )
            );
            let ethereum = native(ETHEREUM, testnet).unwrap();
            assert_eq!(ethereum, capsule(ETHEREUM, testnet).unwrap());
            assert!(ethereum.1);
            let derived = starknet::derive_from_ethereum(
                ETHEREUM,
                safe_cex_venue_paradex::sign::ethereum_chain(testnet),
            )
            .unwrap();
            assert_eq!(ethereum.0, starknet::public_key_bytes(&derived));
        }
        // The two networks derive different Stark keys from one Ethereum key.
        assert_ne!(
            native(ETHEREUM, false).unwrap(),
            native(ETHEREUM, true).unwrap()
        );
    }

    #[test]
    fn every_old_stark_spelling_keeps_its_scalar_and_every_refused_one_stays_refused() {
        let one = native(STARK, false).unwrap();
        let decimal = format!("{}1", "0".repeat(65));
        for accepted in [STARK.to_owned(), STARK.replacen("0x", "0X", 1), decimal] {
            assert_eq!(accepted.len(), 66);
            assert_eq!(native(&accepted, false).unwrap(), one, "{accepted}");
            assert_eq!(capsule(&accepted, false).unwrap(), one, "{accepted}");
        }
        for refused in [
            format!("0x{}", "0".repeat(64)),
            format!("0x{}", "f".repeat(64)),
            "0x0800000000000010ffffffffffffffffb781126dcae7b2321e66a241adc64d2f".into(),
            format!("0x{}", "q".repeat(64)),
            format!(" {}", &STARK[..65]),
            format!("{STARK} "),
            "not-a-key-sentinel".into(),
        ] {
            for error in [native(&refused, false), capsule(&refused, false)] {
                let error = error.err().unwrap();
                assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::InvalidInput));
                assert!(!format!("{error:?}").contains(refused.trim()), "{refused}");
            }
        }
    }

    #[test]
    fn public_intent_ignores_the_key_and_legacy_fields_are_consumed() {
        let mut public = options(Some("not-a-key-sentinel"), false);
        public.public_data_only = true;
        let (_, identities) = crate::prepare_local_signer_for("paradex", &mut public).unwrap();
        assert!(identities.primary.is_none() && !public.paradex_ethereum_key);
        let mut legacy = options(Some(STARK), false);
        legacy.secret = Some("ignored-secret-sentinel".into());
        legacy.passphrase = Some("ignored-passphrase-sentinel".into());
        crate::prepare_local_signer_for("paradex", &mut legacy).unwrap();
        assert!(legacy.secret.is_none() && legacy.passphrase.is_none() && legacy.key.is_none());
    }

    #[test]
    fn missing_public_addresses_leave_the_explicit_private_refusal_first() {
        for field in ["paradexAddress", "subkeyAddress"] {
            let mut value = serde_json::to_value(json!({
                "key": "not-a-key-sentinel",
                "paradexAddress": "0x0123",
                "subkeyAddress": "0x456",
                "publicMode": {"requested": false},
            }))
            .unwrap();
            value[field] = json!("");
            let mut options: EngineOptions = serde_json::from_value(value).unwrap();
            let (_, identities) = crate::prepare_local_signer_for("paradex", &mut options).unwrap();
            assert!(
                identities.primary.is_none() && options.key.is_none(),
                "{field}"
            );
        }
    }

    #[test]
    fn hosted_secondary_duplicate_and_wrong_path_inputs_refuse() {
        let mut hosted = options(Some(STARK), false);
        hosted.payload_signer = true;
        let error = crate::prepare_local_signer_for("paradex", &mut hosted)
            .err()
            .unwrap();
        assert_eq!(error.kind, Some(safe_cex_engine::ErrorKind::Unsupported));
        assert!(hosted.key.is_none());
        for input in [
            json!({"primary": {"secret": STARK, "secretEncoding": "hex"}}),
            json!({"primary": {"secret": STARK}, "secondary": {"secret": STARK}}),
            json!({"primary": {"key": "api", "secret": STARK}}),
        ] {
            let mut options = options(None, false);
            assert!(
                crate::local_signer_from_capsule("paradex", input.to_string(), &mut options)
                    .is_err()
            );
        }
        let mut both = options(Some(STARK), false);
        assert!(
            crate::local_signer_from_capsule(
                "paradex",
                json!({"primary": {"secret": STARK}}).to_string(),
                &mut both
            )
            .is_err()
        );
        let options = options(None, false);
        assert!(
            crate::local_signer_from_json_for_mode(
                "paradex",
                json!({"primary": {"secret": STARK}}).to_string(),
                &options
            )
            .is_err()
        );
    }
}
