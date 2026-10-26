//! Trusted Lighter setup keeps the original key facts and only public lookup metadata.
use crate::{EngineOptions, VenueError, invalid};
use safe_cex_crypto::signer::{LighterPrincipal, LocalSetup, LocalSigner};
use safe_cex_engine::signing::{CredentialSetupPolicy, Identities, Provider};

pub(crate) fn capture_lookup(options: &mut EngineOptions) {
    if options.lighter_l1_address.is_none() {
        options.lighter_l1_address = options
            .key
            .as_deref()
            .filter(|key| {
                key.len() == 42
                    && key.starts_with("0x")
                    && key[2..].bytes().all(|b| b.is_ascii_hexdigit())
            })
            .map(str::to_owned);
    }
}

pub(crate) fn install(
    setup: LocalSetup,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let public = crate::construction::signer_setup_public_data_only(options);
    if !public && options.lighter_signer_provider.as_deref().is_some_and(|provider| !provider.is_empty() && provider != "tealstreet-sdk") { return Err(invalid("Unsupported Lighter signer provider")); }
    let index = if public || setup.primary.is_none() { 0 } else { match options
        .api_key_index
        .as_deref()
        .and_then(|index| index.parse::<u64>().ok())
    {
        Some(index) => index,
        None => return Err(invalid("Lighter requires an API key index in 0..=254")),
    } };
    let principal = LighterPrincipal::new(index)
        .map_err(|_| invalid("Lighter requires an API key index in 0..=254"))?;
    let (signer, identities, _) =
        LocalSigner::from_setup_with_lighter_principal(setup, policy, public, principal)
            .map_err(|_| invalid("Invalid Lighter local signer setup"))?;
    Ok((std::sync::Arc::new(signer), identities))
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
    let setup = serde_json::from_str::<LocalSetup>(&input)
        .map_err(|_| invalid("Invalid Lighter local signer setup"))?;
    install(setup, policy, options)
}
