//! Trusted DELEGATED account construction; socket handover is a separate step.
use crate::accounts::{engine_options, routing_plan};
use crate::envelope::InitPayload;
use safe_cex_engine::signing::Provider;
use serde_json::json;
use zeroize::Zeroizing;

/// A handed-over pair, consumed by trusted construction. Never a socket payload.
pub struct DelegatedPair {
    key: Zeroizing<String>,
    secret: Zeroizing<String>,
    passphrase: Option<Zeroizing<String>>,
    signing_key: Option<Zeroizing<String>>,
}
impl DelegatedPair {
    pub fn new(key: String, secret: String, passphrase: Option<String>) -> Self {
        Self {
            key: Zeroizing::new(key),
            secret: Zeroizing::new(secret),
            passphrase: passphrase.map(Zeroizing::new),
            signing_key: None,
        }
    }
    /// Private key-only material comes exclusively from the encrypted credentials.
    pub fn with_signing_key(mut self, signing_key: String) -> Self {
        self.signing_key = Some(Zeroizing::new(signing_key));
        self
    }

    pub(crate) fn validate(&self, init: &InitPayload) -> Result<(), String> {
        if !crate::delegated::enabled(&init.exchange_name) {
            return Err("DELEGATED is not enabled for this venue".into());
        }
        if init.public_only == Some(true) || init.account_name.starts_with("__pub_") {
            return Err("DELEGATED requires a private account".into());
        }
        if self.key.as_str() != init.key
            || self.key.is_empty()
            || self.key.len() > 1_048_576
            || self.key.contains(['\r', '\n', '\0'])
            || !super::valid_credential_shape(
                &init.exchange_name,
                &self.secret,
                self.signing_key.as_deref().map(String::as_str),
            )
            || self.signing_key.as_ref().is_some_and(|material| {
                material.as_str() == init.key
                    || init.subkey_address.as_deref() != Some(init.key.as_str())
            })
        {
            return Err("Invalid DELEGATED account setup".into());
        }
        Ok(())
    }
}

/// The account's resident provider; material stays in the existing LocalSigner.
/// This handle neither serializes nor makes another copy of the material.
#[derive(Clone)]
pub struct ResidentCopy {
    protection: Option<std::sync::Arc<safe_cex_engine::signing::ProtectedProvider>>,
    #[cfg(test)]
    test_provider: Option<Provider>,
    identities: safe_cex_engine::signing::Identities,
    state: Option<safe_cex_venues::TrustedSignerState>,
}
impl ResidentCopy {
    #[cfg(test)]
    pub(crate) fn test_provider(provider: Provider) -> Self {
        Self {
            protection: None,
            test_provider: Some(provider),
            identities: Default::default(),
            state: None,
        }
    }

    /// Trusted host ingress, as with a remote account's provider.
    pub fn provider(&self) -> Provider {
        #[cfg(test)]
        if let Some(provider) = &self.test_provider {
            return provider.clone();
        }
        self.protection
            .as_ref()
            .expect("canonical protected setup")
            .provider()
    }

    /// Retention/disposal authority; all previously issued engine handles refuse.
    pub fn erase(&self) {
        if let Some(protection) = &self.protection {
            protection.erase();
        }
    }

    pub(crate) fn configure(&self, config: &mut safe_cex_native::ServerConfig) {
        config.trusted_signer_state = self.state.clone();
    }

    pub(crate) fn reconfigure(&self, init: &InitPayload) -> Result<PreparedDelegated, String> {
        if !super::enabled(&init.exchange_name) || init.public_only == Some(true) {
            return Err("DELEGATED settings are not enabled for this account".into());
        }
        let mut options = engine_options(init, false);
        let fields = options.as_object_mut().expect("engine options object");
        fields.remove("key");
        fields.remove("passphrase");
        write_public_options(fields, self.state.as_ref());
        fields.insert("signing".into(), json!(self.identities));
        fields.insert("publicDataOnly".into(), false.into());
        fields.insert("remoteSigning".into(), false.into());
        fields.insert("payloadSigner".into(), false.into());
        Ok(PreparedDelegated {
            plan: routing_plan(init, false)?,
            options: options.to_string(),
            resident: self.clone(),
        })
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
    mut pair: DelegatedPair,
) -> Result<PreparedDelegated, String> {
    pair.validate(init)?;
    let plan = routing_plan(init, false)?;
    let mut options = engine_options(init, false);
    let fields = options.as_object_mut().expect("engine options object");
    if let Some(serde_json::Value::String(key)) = fields.remove("key") {
        drop(Zeroizing::new(key));
    }
    let mut native: safe_cex_venues::EngineOptions = serde_json::from_value(options.clone())
        .map_err(|_| "Invalid DELEGATED account setup".to_owned())?;
    native.key = Some(match pair.signing_key.as_mut() {
        Some(material) => std::mem::take(&mut **material),
        None => std::mem::take(&mut pair.key),
    });
    native.secret = Some(std::mem::take(&mut pair.secret));
    native.passphrase = pair
        .passphrase
        .as_mut()
        .map(|value| std::mem::take(&mut **value));
    let (protection, identities) =
        safe_cex_venues::prepare_protected_local_signer_for(&init.exchange_name, &mut native)
            .map_err(|_| "Invalid DELEGATED account setup".to_owned())?;
    if identities
        .primary
        .as_ref()
        .is_none_or(|identity| identity.signing_ref().is_err())
    {
        return Err("Invalid DELEGATED account setup".into());
    }
    let state = safe_cex_venues::TrustedSignerState::capture(&init.exchange_name, &native);
    let fields = options.as_object_mut().expect("engine options object");
    fields.remove("passphrase");
    write_public_options(fields, Some(&state));
    fields.insert("signing".into(), json!(identities));
    fields.insert("publicDataOnly".into(), false.into());
    fields.insert("remoteSigning".into(), false.into());
    fields.insert("payloadSigner".into(), false.into());
    Ok(PreparedDelegated {
        plan,
        options: options.to_string(),
        resident: ResidentCopy {
            protection: Some(std::sync::Arc::new(protection)),
            #[cfg(test)]
            test_provider: None,
            identities,
            state: Some(state),
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

fn write_public_options(
    fields: &mut serde_json::Map<String, serde_json::Value>,
    state: Option<&safe_cex_venues::TrustedSignerState>,
) {
    fields.remove("key");
    if let Some(state) = state {
        for (name, value) in state.public_options() {
            fields.insert(name.into(), value.into());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn venue_declaration_controls_existing_private_construction() {
        for opted_in in [false, true] {
            crate::delegated::with_test_declaration(
                json!({"venues": {
                    "bybit": {"remoteSigning": {"enabled": false}},
                    "binance": {"remoteSigning": {"enabled": opted_in}}
                }}),
                async {
                    let init = InitPayload {
                        account_name: "fixture".into(),
                        exchange_name: "binance".into(),
                        key: "synthetic-key".into(),
                        ..Default::default()
                    };
                    let pair = DelegatedPair::new(init.key.clone(), "synthetic".into(), None);
                    assert_eq!(prepare_delegated(&init, pair).is_ok(), opted_in);
                },
            )
            .await;
        }
    }
}
