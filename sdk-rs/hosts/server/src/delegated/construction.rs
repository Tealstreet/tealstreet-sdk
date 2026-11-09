//! Trusted DELEGATED account construction; socket handover is a separate step.
use crate::accounts::{engine_options, routing_plan};
use crate::envelope::InitPayload;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::Provider;
use serde_json::json;
use std::sync::Arc;

/// A handed-over pair, consumed by trusted construction. Never a socket payload.
pub struct DelegatedPair {
    key: String,
    secret: Secret,
    passphrase: Option<Secret>,
}
impl DelegatedPair {
    pub fn new(key: String, secret: Secret, passphrase: Option<Secret>) -> Self {
        Self {
            key,
            secret,
            passphrase,
        }
    }
    pub(crate) fn validate(&self, init: &InitPayload) -> Result<(), String> {
        if !safe_cex_remote_signer::delegated_enabled(&init.exchange_name) {
            return Err("DELEGATED is not enabled for this venue".into());
        }
        if init.public_only == Some(true) || init.account_name.starts_with("__pub_") {
            return Err("DELEGATED requires a private account".into());
        }
        if self.key != init.key
            || self.key.is_empty()
            || self.key.len() > 1_048_576
            || self.key.contains(['\r', '\n', '\0'])
        {
            return Err("Invalid DELEGATED account setup".into());
        }
        Ok(())
    }
}

/// The account's resident provider; material stays in the existing LocalSigner.
/// This handle neither serializes nor makes another copy of the material.
pub struct ResidentCopy {
    provider: Provider,
}
impl ResidentCopy {
    #[cfg(test)]
    pub(crate) fn test_provider(provider: Provider) -> Self {
        Self { provider }
    }

    /// Trusted host ingress, as with a remote account's provider.
    pub fn provider(&self) -> Provider {
        self.provider.clone()
    }
}

pub struct PreparedDelegated {
    pub plan: safe_cex_native::RoutingPlan,
    pub options: String,
    pub resident: ResidentCopy,
}

/// Called only after the host has admitted and verified the handover.
/// It performs construction, not remote credential verification.
pub fn prepare_delegated(
    init: &InitPayload,
    pair: DelegatedPair,
) -> Result<PreparedDelegated, String> {
    pair.validate(init)?;
    let plan = routing_plan(init, false)?;
    let setup = LocalSetup {
        primary: Some(LocalIdentity {
            key: Secret::new(pair.key),
            secret: pair.secret,
            passphrase: pair.passphrase,
            ..Default::default()
        }),
        secondary: None,
    };
    let (signer, identities) = LocalSigner::from_setup_with_policy(
        setup,
        safe_cex_venues::signer_setup_policy(&init.exchange_name),
    )
    .map_err(|_| "Invalid DELEGATED account setup".to_owned())?;
    if identities
        .primary
        .as_ref()
        .is_none_or(|identity| identity.signing_ref().is_err())
    {
        return Err("Invalid DELEGATED account setup".into());
    }
    let mut options = engine_options(init, false);
    let fields = options.as_object_mut().expect("engine options object");
    fields.remove("key");
    fields.remove("passphrase");
    fields.insert("signing".into(), json!(identities));
    fields.insert("publicDataOnly".into(), false.into());
    fields.insert("remoteSigning".into(), false.into());
    fields.insert("payloadSigner".into(), false.into());
    Ok(PreparedDelegated {
        plan,
        options: options.to_string(),
        resident: ResidentCopy {
            provider: Arc::new(signer),
        },
    })
}

/// Separate pending init incarnations without changing the legacy fingerprint.
pub(crate) fn fingerprint(init: &InitPayload) -> String {
    let mut fields: serde_json::Value =
        serde_json::from_str(&init.fingerprint()).expect("fingerprint object");
    fields["hostedSigningMode"] = "delegated".into();
    fields.to_string()
}
