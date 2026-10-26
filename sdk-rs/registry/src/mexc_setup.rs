//! Trusted compatibility mapping before provider normalization; never venue execution.
use super::*;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, Secret};

fn mapping(key: &str) -> String {
    let hash = safe_cex_crypto::sha256(key.as_bytes());
    format!(
        "mexc:{}",
        safe_cex_crypto::encode(&hash[..12], safe_cex_crypto::Encoding::Hex)
    )
}
pub(super) fn raw(
    options: &mut EngineOptions,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    let primary = if options.key.is_some() || options.secret.is_some() {
        let key = zeroize::Zeroizing::new(options.key.take().unwrap_or_default());
        Some(LocalIdentity {
            stable_id: (!key.is_empty()).then(|| mapping(&key)),
            key: Secret::new(key.to_string()),
            secret: Secret::new(options.secret.take().unwrap_or_default()),
            ..Default::default()
        })
    } else {
        None
    };
    let secondary = options
        .read_credentials
        .take()
        .map(|mut read| {
            let valid = read.validate().is_ok();
            let identity = LocalIdentity {
                key: Secret::new(std::mem::take(&mut read.key)),
                secret: Secret::new(std::mem::take(&mut read.secret)),
                ..Default::default()
            };
            read.erase_secrets();
            if valid {
                Ok(identity)
            } else {
                Err(invalid("Invalid MEXC read credentials"))
            }
        })
        .transpose()?;
    local_signer_from_setup(
        LocalSetup { primary, secondary },
        construction::signer_setup_policy("mexc"),
        construction::signer_setup_public_data_only(options),
    )
}
pub(super) fn json(
    input: String,
    public: bool,
) -> Result<
    (
        safe_cex_engine::signing::Provider,
        safe_cex_engine::signing::Identities,
    ),
    VenueError,
> {
    let input = zeroize::Zeroizing::new(input);
    let mut setup: LocalSetup =
        serde_json::from_str(&input).map_err(|_| invalid("Invalid MEXC local signer setup"))?;
    #[derive(Deserialize)]
    struct View<'a> {
        #[serde(borrow)]
        primary: Option<&'a serde_json::value::RawValue>,
    }
    #[derive(Deserialize)]
    struct Key {
        key: Option<String>,
    }
    let view: View<'_> =
        serde_json::from_str(&input).map_err(|_| invalid("Invalid MEXC local signer setup"))?;
    if let Some(primary) = setup.primary.as_mut() {
        let key: Key = serde_json::from_str(
            view.primary
                .ok_or_else(|| invalid("Invalid MEXC primary mapping"))?
                .get(),
        )
        .map_err(|_| invalid("Invalid MEXC primary mapping"))?;
        if let Some(key) = key.key.filter(|key| !key.is_empty()) {
            let key = zeroize::Zeroizing::new(key);
            let expected = mapping(&key);
            if primary.stable_id.as_ref().is_some_and(|id| *id != expected) {
                return Err(invalid(
                    "MEXC primary account identity mapping conflicts with its credential",
                ));
            }
            primary.stable_id = Some(expected);
        }
    }
    local_signer_from_setup(setup, construction::signer_setup_policy("mexc"), public)
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::signing::{Context, Operation, SignReceipt, SignRequest};
    fn present(
        provider: &safe_cex_engine::signing::Provider,
        key: &safe_cex_engine::signing::KeyRef,
    ) -> String {
        let result = provider
            .sign_ready(&SignRequest {
                receipt: SignReceipt::default(),
                context: Context::default(),
                operations: vec![Operation::Present { key: key.clone() }],
            })
            .unwrap()
            .unwrap();
        String::from_utf8(result.values[0].clone()).unwrap()
    }
    #[test]
    fn raw_and_json_setup_preserve_identical_raw_namespaces_before_trim() {
        for (key, stable) in [
            ("replay-key", "mexc:a7c5c5f5cbc33ea500a4850d"),
            (" replay-key ", "mexc:ae0c5396072d0d2a609af843"),
            ("\u{85}replay-key\u{85}", "mexc:7650b0f09e155dbcc2aa6016"),
            (
                "\u{feff}replay-key\u{feff}",
                "mexc:cdf888fd46e388a15151113d",
            ),
        ] {
            let mut options: EngineOptions = serde_json::from_value(
                serde_json::json!({"key":key,"secret":"fixture-secret","publicDataOnly":false}),
            )
            .unwrap();
            let (provider, refs) = raw(&mut options).unwrap();
            let primary = refs.primary.unwrap();
            assert_eq!(primary.require_stable_id(), Ok(stable));
            assert_eq!(present(&provider, &primary.credential), key.trim());
            assert!(options.key.is_none() && options.secret.is_none());
            let (provider, refs) = json(
                serde_json::json!({"primary":{"key":key,"secret":"fixture-secret"}}).to_string(),
                false,
            )
            .unwrap();
            let primary = refs.primary.unwrap();
            assert_eq!(primary.require_stable_id(), Ok(stable));
            assert_eq!(present(&provider, &primary.credential), key.trim());
        }
    }
    #[test]
    fn conflicting_or_invalid_primary_metadata_is_not_silently_rewritten() {
        for stable in ["other-account", "", "bad\naccount"] {
            assert!(json(serde_json::json!({"primary":{"key":"replay-key","secret":"fixture-secret","stableId":stable}}).to_string(),false).is_err());
        }
        assert!(json(serde_json::json!({"primary":{"key":"replay-key","secret":"fixture-secret","stableId":"mexc:a7c5c5f5cbc33ea500a4850d"}}).to_string(),false).is_ok());
    }
    #[test]
    fn credential_only_primary_and_secondary_keep_separate_authority() {
        let (provider,refs)=json(serde_json::json!({"primary":{"key":"replay-key"},"secondary":{"key":" read ","secret":"secondary-secret","stableId":"read-account"}}).to_string(),false).unwrap();
        let primary = refs.primary.unwrap();
        let read = refs.secondary.unwrap();
        assert!(primary.signing_ref().is_err());
        assert!(read.signing_ref().is_ok());
        assert_eq!(
            primary.require_stable_id(),
            Ok("mexc:a7c5c5f5cbc33ea500a4850d")
        );
        assert_eq!(present(&provider, &read.credential), " read ");
        assert_ne!(primary.stable_id, read.stable_id);
    }
}
