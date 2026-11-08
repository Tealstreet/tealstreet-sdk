use super::{EngineOptions, VenueError, invalid};
use safe_cex_engine::signing::{Algorithm, Capability, Identities, Provider};

pub(super) fn injected(
    options: &mut EngineOptions,
    provider: Option<Provider>,
) -> Result<(Option<Provider>, Identities), VenueError> {
    let ids = options
        .signing
        .take()
        .ok_or_else(|| invalid("Orderly injected identities are required"))?;
    let primary = ids
        .primary
        .as_ref()
        .ok_or_else(|| invalid("Orderly primary identity is required"))?;
    primary.validate().map_err(invalid)?;
    let provider = provider.ok_or_else(|| invalid("Orderly injected provider is required"))?;
    let key = options.key.as_deref().unwrap_or_default();
    let key = key.strip_prefix("ed25519:").unwrap_or(key);
    let public_valid = (32..=44).contains(&key.len())
        && bs58::decode(key)
            .into_vec()
            .is_ok_and(|bytes| bytes.len() == 32);
    if !public_valid
        || ids.secondary.is_some()
        || primary.passphrase.is_some()
        || !primary.extra.is_empty()
        || options.read_credentials.is_some()
        || options
            .secret
            .as_deref()
            .is_some_and(|secret| !secret.is_empty())
        || options
            .passphrase
            .as_deref()
            .is_some_and(|passphrase| !passphrase.is_empty())
        || provider.can_sign(
            primary
                .signing_ref()
                .map_err(|_| invalid("Orderly signing ref is required"))?,
            Algorithm::Ed25519,
        ) != Capability::Supported
    {
        return Err(invalid("Invalid Orderly injected signer metadata"));
    }
    options.key = Some(format!("ed25519:{key}"));
    Ok((Some(provider), ids))
}
