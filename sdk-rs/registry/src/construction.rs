//! Synchronous SDK and native construction share these declarative conditions.
use serde::Deserialize;

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
}

include!(concat!(env!("OUT_DIR"), "/construction_rules.rs"));

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
    let present = |value: &Option<String>| value.as_ref().is_some_and(|value| !value.is_empty());
    let matches = |conditions: Conditions, can_sign: bool| {
        conditions.iter().all(|group| {
            group.iter().any(|input| match input {
                Input::Key => present(&options.key),
                Input::Secret => present(&options.secret),
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
                        use safe_cex_venue_asterdex::sign::Version;
                        if options.aster_api_version() == Version::V3 {
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
