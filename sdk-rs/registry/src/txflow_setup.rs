//! Trusted local agent setup. Account addresses stay public; only the provider owns the scalar.
use crate::{EngineOptions, VenueError, invalid};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{CredentialSetupPolicy, Identities, KeyPresence, Provider};

const INVALID: &str = "Invalid TxFlow local signer setup";

// Same raw-intent rule as lighter_setup::setup_public_mode, before provider installation.
fn setup_public_mode(options: &EngineOptions) -> bool {
    options
        .public_mode
        .as_ref()
        .and_then(|mode| mode.requested)
        .unwrap_or(options.public_data_only)
}

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
    venue: &str,
    options: &mut EngineOptions,
    policy: CredentialSetupPolicy,
) -> Result<(Provider, Identities), VenueError> {
    let public = setup_public_mode(options);
    let payload_rule = crate::payload_signing_rules_in_mode(venue, options, false, public);
    drop(options.passphrase.take().map(Secret::new));
    let secret = options.secret.take().map(Secret::new);
    payload_rule?;
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
    venue: &str,
    input: String,
    policy: CredentialSetupPolicy,
    options: &EngineOptions,
) -> Result<(Provider, Identities), VenueError> {
    let input = zeroize::Zeroizing::new(input);
    crate::payload_signing_rules_in_mode(venue, options, false, setup_public_mode(options))?;
    let setup = serde_json::from_str::<LocalSetup>(&input).map_err(|_| invalid(INVALID))?;
    install(setup, policy, options)
}
