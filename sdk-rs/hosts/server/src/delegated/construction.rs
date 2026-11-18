//! Trusted DELEGATED account construction; socket handover is a separate step.
use crate::accounts::{engine_options, routing_plan};
use crate::envelope::InitPayload;
use serde_json::json;
use zeroize::Zeroizing;

/// A handed-over pair, consumed by trusted construction. Never a socket payload.
pub struct DelegatedPair {
    key: Zeroizing<String>,
    secret: Zeroizing<String>,
    passphrase: Option<Zeroizing<String>>,
    signing_key: Option<Zeroizing<String>>,
    omni_key_seed: Option<Zeroizing<String>>,
}
impl DelegatedPair {
    pub fn new(key: String, secret: String, passphrase: Option<String>) -> Self {
        Self {
            key: Zeroizing::new(key),
            secret: Zeroizing::new(secret),
            passphrase: passphrase.map(Zeroizing::new),
            signing_key: None,
            omni_key_seed: None,
        }
    }
    /// Private key-only material comes exclusively from the encrypted credentials.
    pub fn with_signing_key(mut self, signing_key: String) -> Self {
        self.signing_key = Some(Zeroizing::new(signing_key));
        self
    }

    /// ApeX signing material comes exclusively from the encrypted handover.
    pub fn with_omni_key_seed(mut self, omni_key_seed: String) -> Self {
        self.omni_key_seed = Some(Zeroizing::new(omni_key_seed));
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
                self.omni_key_seed.as_deref().map(String::as_str),
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

use std::sync::Arc;
mod resident;
pub use resident::ResidentCopy;

impl ResidentCopy {
    pub(crate) fn configure(&self, config: &mut safe_cex_native::ServerConfig) {
        config.trusted_signer_state = self.state.clone();
        config.trusted_signer_identities = self.trusted_identities.clone();
    }

    pub(crate) fn reconfigure(
        self: &Arc<Self>,
        init: &InitPayload,
    ) -> Result<PreparedDelegated, String> {
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
    pub resident: Arc<ResidentCopy>,
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
    native.omni_key_seed = pair
        .omni_key_seed
        .as_mut()
        .map(|value| std::mem::take(&mut **value));
    let (protection, identities) =
        safe_cex_venues::prepare_protected_local_signer_for(&init.exchange_name, &mut native)
            .map_err(|_| "Invalid DELEGATED account setup".to_owned())?;
    if !protection.memory_locked() {
        crate::delegated_memory::warn_locking_unavailable();
    }
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
    let mut resident = ResidentCopy::new(protection, identities.clone(), Some(state));
    resident.trusted_identities = Some(safe_cex_native::TrustedSignerIdentities::capture(
        &init.exchange_name,
        resident.provider(),
        identities,
    ));
    Ok(PreparedDelegated {
        plan,
        options: options.to_string(),
        resident: Arc::new(resident),
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
    #[cfg(target_os = "linux")]
    #[tokio::test]
    async fn encrypted_apex_seed_installs_the_protected_extra_role() {
        use safe_cex_engine::signing::{Algorithm, Capability, ExtraKey};
        crate::delegated::with_test_declaration(
            json!({"venues":{"apexomni":{"remoteSigning":{"enabled":true}}}}),
            async {
                let init = InitPayload {
                    account_name: "synthetic-apex".into(),
                    exchange_name: "apexomni".into(),
                    key: "synthetic-apex-key".into(),
                    ..Default::default()
                };
                let monitor = prepare_delegated(
                    &init,
                    DelegatedPair::new(init.key.clone(), "synthetic-apex-secret".into(), None),
                )
                .unwrap();
                assert!(
                    !monitor
                        .resident
                        .identities()
                        .primary
                        .as_ref()
                        .unwrap()
                        .extra
                        .contains_key(&ExtraKey::ZklinkSeed)
                );
                let seed = format!("0x{}", "11".repeat(65));
                let mut owner = crate::delegated::Handovers::default();
                let offer = owner
                    .issue(&init.account_name, &init.exchange_name, 100)
                    .unwrap();
                let bytes = serde_json::to_vec(&json!({
                "key":init.key, "secret":"synthetic-apex-secret", "passphrase":"synthetic-phrase",
                "omniKeySeed":seed
            })).unwrap();
                let sealed =
                    crate::delegated::tests::seal(&offer, &init.exchange_name, &init.key, &bytes);
                let opened = owner
                    .take(&init.account_name, &sealed, 101)
                    .unwrap()
                    .open(&init.exchange_name, &init.key, &sealed, 102)
                    .expect("the encrypted ApeX seed must survive the credential schema");
                let prepared = prepare_delegated(&init, opened.pair()).unwrap();
                let primary = prepared.resident.identities().primary.as_ref().unwrap();
                let extra = primary
                    .extra
                    .get(&ExtraKey::ZklinkSeed)
                    .expect("canonical ApeX setup must install the seed role")
                    .clone();
                let provider = prepared.resident.provider();
                assert_eq!(
                    provider.can_sign(&extra, Algorithm::ZkLinkContract),
                    Capability::Supported
                );
                let options: serde_json::Value = serde_json::from_str(&prepared.options).unwrap();
                for field in ["key", "secret", "passphrase", "omniKeySeed", "signingKey"] {
                    assert!(
                        options.get(field).is_none(),
                        "private field in public engine options: {field}"
                    );
                }
                assert!(!prepared.options.contains(&seed));
                let reconfigured = prepared.resident.reconfigure(&init).unwrap();
                assert_eq!(
                    reconfigured.resident.identities(),
                    prepared.resident.identities()
                );
                assert!(std::sync::Arc::ptr_eq(
                    &provider,
                    &reconfigured.resident.provider()
                ));
                assert!(!reconfigured.options.contains(&seed));
                prepared.resident.erase();
                assert_eq!(
                    provider.can_sign(&extra, Algorithm::ZkLinkContract),
                    Capability::Unsupported
                );
            },
        )
        .await;
    }

    #[tokio::test]
    async fn encrypted_apex_seed_rejects_foreign_layouts_and_invalid_material() {
        for (venue, seed) in [
            ("bybit", format!("0x{}", "11".repeat(65))),
            ("hyperliquid", format!("0x{}", "11".repeat(65))),
            ("paradex", format!("0x{}", "11".repeat(65))),
            ("apexomni", String::new()),
            ("apexomni", "0x01".to_owned()),
            ("apexomni", format!("0x{}", "gg".repeat(65))),
            ("apexomni", format!("0x{}", "11".repeat(64))),
        ] {
            crate::delegated::with_test_declaration(
                json!({"venues":{venue:{"remoteSigning":{"enabled":true}}}}),
                async {
                    let mut owner = crate::delegated::Handovers::default();
                    let offer = owner.issue("synthetic-apex", venue, 100).unwrap();
                    let bytes = serde_json::to_vec(&json!({
                        "key":"synthetic-key", "secret":"synthetic-secret", "omniKeySeed":seed
                    }))
                    .unwrap();
                    let sealed =
                        crate::delegated::tests::seal(&offer, venue, "synthetic-key", &bytes);
                    assert!(
                        owner
                            .take("synthetic-apex", &sealed, 101)
                            .unwrap()
                            .open(venue, "synthetic-key", &sealed, 102)
                            .is_err()
                    );
                },
            )
            .await;
        }
    }

    #[tokio::test]
    async fn apex_seed_schema_and_pair_preserve_private_material() {
        crate::delegated::with_test_declaration(
        json!({"venues":{"apexomni":{"remoteSigning":{"enabled":true}}}}),
        async {
            let seed = format!("0x{}", "11".repeat(65));
            let raw = serde_json::to_string(&json!({"key":"synthetic-apex-key", "secret":"synthetic-apex-secret", "passphrase":"synthetic-phrase", "omniKeySeed":seed})).unwrap();
            let candidate: crate::delegated::Credentials = serde_json::from_str(&raw)
                .expect("ApeX seed must survive the encrypted credential schema");
            assert!(candidate.omni_key_seed.as_deref().map(String::as_str) == Some(seed.as_str()));
            let pair = candidate.pair();
            assert!(pair.omni_key_seed.as_deref().map(String::as_str) == Some(seed.as_str()));
            let mut init = InitPayload { account_name:"synthetic-apex".into(), exchange_name:"apexomni".into(), key:"synthetic-apex-key".into(), ..Default::default() };
            assert!(pair.validate(&init).is_ok());
            let mixed = candidate.pair().with_signing_key("synthetic-wallet-key".into());
            assert!(mixed.validate(&init).is_err());
            init.exchange_name = "bybit".into();
            assert!(pair.validate(&init).is_err());
        },
    ).await;
    }
}
