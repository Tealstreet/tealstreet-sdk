//! Preserve legacy construction refusals before provider setup consumes credentials.
use super::*;
use safe_cex_crypto::signer::LocalSetup;
use safe_cex_engine::signing::{Identities, Provider};
use std::borrow::Cow;

pub(super) fn preflight(
    options: &mut EngineOptions,
    public_data_only: bool,
) -> Result<(), VenueError> {
    // A remote-signing flag is not a provider. Missing raw credentials must
    // keep the native refusal before a malformed orphan secret is decoded.
    if !public_data_only {
        local_signing_rules("kraken", options)?;
    }
    validate_read_key(construction::ReadCredentialsPolicy::Install, options)?;
    let present = |value: &Option<String>| value.as_deref().is_some_and(|text| !text.is_empty());
    if !public_data_only
        && present(&options.key)
        && let Some(secret) = options.secret.as_deref()
    {
        safe_cex_venue_kraken::sign::Signer::new(secret).map_err(invalid)?;
    }
    if public_data_only
        || (!options.remote_signing && (options.key.is_none() || options.secret.is_none()))
    {
        // Public startup ignores unused primary material, including malformed
        // base64. Erase it before the generic provider decoder can observe it.
        for field in [
            &mut options.key,
            &mut options.secret,
            &mut options.passphrase,
        ] {
            safe_cex_crypto::secrets::erase(field);
        }
    }
    Ok(())
}

pub(super) fn json(
    input: String,
    public_data_only: bool,
) -> Result<(Provider, Identities), VenueError> {
    let input = zeroize::Zeroizing::new(input);
    // Retain the generic private-capsule size and schema limits. Owned secret
    // fields and the original buffer are wiped on every exit.
    if input.len() > 4 * 1024 * 1024 {
        return Err(invalid("Invalid local signer setup"));
    }
    let mut setup: LocalSetup =
        serde_json::from_str(&input).map_err(|_| invalid("Invalid local signer setup"))?;
    if public_data_only {
        setup.primary = None;
    } else {
        // Borrow only the primary material for the venue's original diagnostic;
        // metadata validation and actual custody remain in the generic provider.
        #[derive(Deserialize)]
        struct View<'a> {
            #[serde(borrow)]
            primary: Option<Primary<'a>>,
        }
        #[derive(Deserialize)]
        struct Primary<'a> {
            #[serde(borrow)]
            key: Option<Cow<'a, str>>,
            #[serde(borrow)]
            secret: Option<Cow<'a, str>>,
        }
        impl Drop for Primary<'_> {
            fn drop(&mut self) {
                use zeroize::Zeroize;
                for field in [&mut self.key, &mut self.secret] {
                    if let Some(Cow::Owned(text)) = field {
                        text.zeroize();
                    }
                }
            }
        }
        let view: View<'_> =
            serde_json::from_str(&input).map_err(|_| invalid("Invalid local signer setup"))?;
        if let Some(primary) = view.primary
            && primary.key.as_deref().is_some_and(|key| !key.is_empty())
            && let Some(secret) = primary
                .secret
                .as_deref()
                .filter(|secret| !secret.is_empty())
        {
            safe_cex_venue_kraken::sign::Signer::new(secret).map_err(invalid)?;
        }
    }
    let (provider, identities) = local_signer_from_setup(
        setup,
        construction::signer_setup_policy("kraken"),
        public_data_only,
    )?;
    admit_json_setup("kraken", provider, identities, public_data_only)
}
