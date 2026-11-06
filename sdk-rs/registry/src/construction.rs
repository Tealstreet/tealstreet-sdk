//! Synchronous SDK and native construction share these declarative conditions.
use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SetupFieldPolicy};
use safe_cex_engine::signing_policy::{RemoteSigningPolicy, SignatureEncoding};
use serde::{Deserialize, Serialize};

use crate::EngineOptions;

type Conditions = &'static [&'static [Input]];

#[derive(Clone, Copy)]
enum Input {
    Key,
    Secret,
    RemoteSigner,
    PayloadSigner,
    ApplicationId,
    Vault,
    ParadexAddress,
    SubkeyAddress,
    ApiKeyIndex,
    AsterLocalAuth,
    CanSign,
}

struct Rule {
    can_sign: Option<Conditions>,
    private_eligible: Option<Conditions>,
    explicit_private_error: Option<&'static str>,
    bridge: BridgeRule,
    signer_setup: CredentialSetupPolicy,
    remote_signing: RemoteSigningPolicy,
    // Read only by builds that compile a mapped family.
    #[cfg_attr(not(feature = "asterdex"), allow(dead_code))]
    setup_mapping: Option<SetupMapping>,
}

/// A family whose trusted setup maps account fields before provider install.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SetupMapping {
    /// Existing raw-Primary account namespace, before credential trimming.
    Mexc,
    /// Version-resolved V1/V3 slots and the read agent's primary-wallet alias.
    Asterdex,
    /// The OmniKey seed as the primary's zkLink role; no read credentials.
    ApexOmni,
    Lighter,
    Hyperliquid,
    Paradex,
}

struct BridgeRule {
    spot: bool,
    demo: DemoRule,
    primary_credentials_only: bool,
    payload_signing: bool,
    api_key_index_required: bool,
    isolated_book_subscriptions: bool,
    dispatch_signing: bool,
    read_credentials: ReadCredentialsPolicy,
}

/// What trusted setup does with a family's read key (declared, never inferred).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum ReadCredentialsPolicy {
    /// Installed as the secondary identity, as before.
    #[default]
    Install,
    /// Validated once, then dropped; only the result reaches the venue.
    ValidateOnly,
    /// Also usable without a primary in public-only mode, for Read roles only.
    PublicRead,
}

impl ReadCredentialsPolicy {
    fn is_install(&self) -> bool {
        *self == Self::Install
    }
}

enum DemoRule {
    Never,
    Always,
    HyroMode,
}

/// Synchronous bridge choices, resolved from the same rules before WASM loads.
#[derive(Default, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeConstruction {
    pub spot: bool,
    pub demo: bool,
    pub primary_credentials_only: bool,
    pub payload_signing: bool,
    pub api_key_index_required: bool,
    pub isolated_book_subscriptions: bool,
    pub dispatch_signing: bool,
    /// Omitted when `Install`, so undeclared families keep their exact record.
    #[serde(skip_serializing_if = "ReadCredentialsPolicy::is_install")]
    pub read_credentials: ReadCredentialsPolicy,
}

pub fn bridge(venue: &str, options: &EngineOptions) -> BridgeConstruction {
    let rule = &venue_rule(venue).unwrap_or(&DEFAULT_RULE).bridge;
    let demo = match rule.demo {
        DemoRule::Never => false,
        DemoRule::Always => true,
        DemoRule::HyroMode => {
            #[cfg(feature = "bybit")]
            {
                options
                    .hyrotrader
                    .as_ref()
                    .is_some_and(|config| config.is_demo())
            }
            #[cfg(not(feature = "bybit"))]
            {
                let _ = options;
                false
            }
        }
    };
    BridgeConstruction {
        spot: rule.spot,
        demo,
        primary_credentials_only: rule.primary_credentials_only,
        payload_signing: rule.payload_signing,
        api_key_index_required: rule.api_key_index_required,
        isolated_book_subscriptions: rule.isolated_book_subscriptions,
        dispatch_signing: rule.dispatch_signing,
        read_credentials: rule.read_credentials,
    }
}

include!(concat!(env!("OUT_DIR"), "/construction_rules.rs"));

/// The declared family setup mapping, if any.
#[cfg_attr(not(feature = "asterdex"), allow(dead_code))]
pub fn signer_setup_mapping(venue: &str) -> Option<SetupMapping> {
    venue_rule(venue).and_then(|rule| rule.setup_mapping)
}

/// The family's declared read-key policy.
pub fn read_credentials_policy(venue: &str) -> ReadCredentialsPolicy {
    #[cfg(test)]
    if let Some(policy) = test_policy::get(venue) {
        return policy;
    }
    venue_rule(venue)
        .unwrap_or(&DEFAULT_RULE)
        .bridge
        .read_credentials
}

/// Family declarations are compiled from the shared construction table.
pub fn signer_setup_policy(venue: &str) -> CredentialSetupPolicy {
    venue_rule(venue).unwrap_or(&DEFAULT_RULE).signer_setup
}

pub fn remote_signing_policy(venue: &str) -> RemoteSigningPolicy {
    venue_rule(venue).unwrap_or(&DEFAULT_RULE).remote_signing
}

/// Trusted setup uses explicit caller intent before keys are stripped/resolved.
/// An explicit shared publicMode request takes precedence over the legacy flag.
pub fn signer_setup_public_data_only(options: &EngineOptions) -> bool {
    options
        .public_mode
        .as_ref()
        .map_or(options.public_data_only, |mode| {
            mode.requested == Some(true)
        })
}

/// Raw caller intent, supplied only by SDKs that use shared construction rules.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicMode {
    pub requested: Option<bool>,
    #[serde(default)]
    remote_signer: bool,
    #[serde(default)]
    payload_signer: bool,
    #[serde(default)]
    api_key_index_present: bool,
}

pub struct Resolved {
    pub can_sign: bool,
    pub public_data_only: bool,
    pub explicit_private_error: Option<&'static str>,
}

pub fn resolve(venue: &str, options: &EngineOptions) -> Option<Resolved> {
    let mode = options.public_mode.as_ref()?;
    let rule = venue_rule(venue);
    let identities = options
        .signing
        .as_ref()
        .filter(|_| bridge(venue, options).dispatch_signing);
    let present = |value: &Option<String>| value.as_ref().is_some_and(|value| !value.is_empty());
    let matches = |conditions: Conditions, can_sign: bool| {
        conditions.iter().all(|group| {
            group.iter().any(|input| match input {
                Input::Key => {
                    present(&options.key)
                        || identities
                            .and_then(|s| s.primary.as_ref())
                            .is_some_and(|id| !id.credential.0.is_empty())
                }
                Input::Secret => {
                    present(&options.secret)
                        || identities
                            .and_then(|s| s.primary.as_ref())
                            .is_some_and(|id| id.signing_opt().is_some_and(|key| !key.0.is_empty()))
                }
                Input::RemoteSigner => mode.remote_signer,
                Input::PayloadSigner => mode.payload_signer,
                Input::ApplicationId => present(&options.application_id),
                Input::Vault => present(&options.vault),
                Input::ParadexAddress => present(&options.paradex_address),
                Input::SubkeyAddress => present(&options.subkey_address),
                Input::ApiKeyIndex => mode.api_key_index_present,
                Input::CanSign => can_sign,
                Input::AsterLocalAuth => {
                    #[cfg(feature = "asterdex")]
                    {
                        use safe_cex_venue_asterdex::sign::{Version, is_address};
                        // After trusted setup: the installed wallet ref plus the
                        // public V3 wallets setup resolved.
                        if let Some(identities) = identities {
                            let address = |value: &Option<String>| {
                                value.as_deref().is_some_and(|text| is_address(text.trim()))
                            };
                            identities.primary.as_ref().is_some_and(|id| {
                                id.signing_opt().is_some_and(|key| !key.0.is_empty())
                            }) && (options.aster_api_version() == Version::V1
                                || (address(&options.asterdex.signer_address)
                                    && (address(&options.asterdex.user_address)
                                        || address(&options.application_id))))
                        } else if options.aster_api_version() == Version::V3 {
                            options.aster_version(None) == Version::V3
                        } else {
                            present(&options.secret)
                        }
                    }
                    #[cfg(not(feature = "asterdex"))]
                    {
                        false
                    }
                }
            })
        })
    };
    let can_sign = matches(
        rule.and_then(|rule| rule.can_sign)
            .or(DEFAULT_RULE.can_sign)
            .expect("default signing conditions"),
        false,
    );
    let public_data_only = mode.requested == Some(true)
        || !matches(
            rule.and_then(|rule| rule.private_eligible)
                .or(DEFAULT_RULE.private_eligible)
                .expect("default private conditions"),
            can_sign,
        );
    let explicit_private_error = if mode.requested == Some(false) && public_data_only {
        rule.and_then(|rule| rule.explicit_private_error)
    } else {
        None
    };
    Some(Resolved {
        can_sign,
        public_data_only,
        explicit_private_error,
    })
}

/// Test-only declaration for one family, since no shipped row declares a policy.
#[cfg(test)]
pub(crate) mod test_policy {
    use super::ReadCredentialsPolicy;
    use std::cell::Cell;
    thread_local! {
        static DECLARED: Cell<Option<(&'static str, ReadCredentialsPolicy)>> = const { Cell::new(None) };
    }
    pub(crate) fn get(venue: &str) -> Option<ReadCredentialsPolicy> {
        DECLARED
            .get()
            .filter(|(declared, _)| *declared == venue)
            .map(|(_, policy)| policy)
    }
    /// Declares `policy` for `venue` until the guard drops. The wiring tests
    /// that use it need a compiled dispatch family (WEEX).
    #[cfg_attr(not(any(feature = "weex", feature = "backpack")), allow(dead_code))]
    pub(crate) fn declare(venue: &'static str, policy: ReadCredentialsPolicy) -> impl Drop {
        struct Guard;
        impl Drop for Guard {
            fn drop(&mut self) {
                DECLARED.set(None);
            }
        }
        DECLARED.set(Some((venue, policy)));
        Guard
    }
}
