//! Native remote signing for the hosted exchange server. The server holds no
//! secrets: each engine sign request travels to the client's device as today's
//! ws-auth `sign-req` and resolves on its `sign-res`. Mirrors the TS
//! `DelegatedSignerSetup` + `createRemoteSignerProvider` + `WebSocketRemoteSigner`.
mod link;
mod policy;
mod provider;

use std::sync::Arc;

use safe_cex_engine::signing::{KeyRef, Provider};
use serde_json::{Map, Value, json};

pub use link::{
    ECHO_PAD_CAP, EchoSize, MAX_RESPONSE_FIELD, Outbox, SIGN_TIMEOUT, SignerLink, SocketId,
};
pub use provider::{FATAL_MIN_STREAK, MAX_CONSECUTIVE_FAILURES, RemoteProvider};

use policy::PassphraseRule;
use provider::{Inner, Owned};

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SetupError(pub &'static str);
impl std::fmt::Display for SetupError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.0)
    }
}
impl std::error::Error for SetupError {}

/// One hosted account's signer and the engine options to construct it with.
pub struct RemoteAccount {
    pub provider: Provider,
    /// Engine options without credentials, carrying the opaque signing
    /// identities, `remoteSigning: true` and the raw public-mode intent.
    pub options_json: String,
    inner: Arc<Inner>,
}

impl RemoteAccount {
    /// Called once after `MAX_CONSECUTIVE_FAILURES` failures spanning at least
    /// `FATAL_MIN_STREAK`; the server disposes the account, as today.
    pub fn on_fatal(&self, callback: impl FnOnce() + Send + 'static) {
        self.inner.set_fatal(Box::new(callback));
    }
}

/// Fields that are signing material or credential slots, never engine options.
const CREDENTIAL_FIELDS: [&str; 8] = [
    "key",
    "secret",
    "passphrase",
    "secondary_key",
    "secondary_secret",
    "secondary_passphrase",
    "localSigner",
    "omniKeySeed",
];

fn reference(slot: &str) -> KeyRef {
    let mut bytes = [0u8; 12];
    getrandom::getrandom(&mut bytes).expect("OS entropy");
    let id: String = bytes.iter().map(|byte| format!("{byte:02x}")).collect();
    KeyRef(format!("delegated/{id}/{slot}"))
}

/// Build the keyless provider for `venue` from the engine options the server
/// would otherwise construct it with (the account's public `key`, optional
/// `passphrase`, and every non-secret field). `secret` must be absent or empty.
pub fn remote_account(
    link: Arc<SignerLink>,
    venue: &str,
    account_name: Option<String>,
    engine_options: &str,
) -> Result<RemoteAccount, SetupError> {
    let policy = policy::policy(venue)
        .filter(|policy| policy.enabled)
        .ok_or(SetupError("Remote signing is not supported for this venue"))?;
    let Ok(Value::Object(mut options)) = serde_json::from_str::<Value>(engine_options) else {
        return Err(SetupError("Invalid engine options"));
    };
    let text = |options: &Map<String, Value>, field: &str| {
        options
            .get(field)
            .and_then(Value::as_str)
            .map(str::to_owned)
    };
    if text(&options, "secret").is_some_and(|secret| !secret.is_empty()) {
        return Err(SetupError("A remote-signed account holds no secret"));
    }
    if ["secondary_key", "secondary_secret", "secondary_passphrase"]
        .iter()
        .any(|field| options.get(*field).is_some_and(|value| !value.is_null()))
    {
        return Err(SetupError("Secondary remote signing is not supported"));
    }
    let mut key =
        text(&options, "key").ok_or(SetupError("A remote-signed account needs its public key"))?;
    if policy.trim_credential {
        key = key.trim().to_owned();
    }
    if key.is_empty() || key.contains(['\r', '\n', '\0']) || key.len() > 1_048_576 {
        return Err(SetupError("Invalid public key"));
    }
    let requested = options
        .get("publicMode")
        .and_then(|mode| mode.get("requested"))
        .cloned()
        .or_else(|| options.get("publicDataOnly").cloned())
        .filter(Value::is_boolean)
        .unwrap_or(Value::Null);
    let public_only = requested == Value::Bool(true);
    let passphrase = text(&options, "passphrase").filter(|value| !value.is_empty());
    if passphrase.is_none()
        && policy
            .passphrase
            .is_some_and(|rule| rule != PassphraseRule::Optional)
        && !(policy.passphrase == Some(PassphraseRule::RequiredForPrivate) && public_only)
    {
        return Err(SetupError(
            "Rust engine primary signing credentials require a passphrase",
        ));
    }
    let passphrase = match (&policy.passphrase_header, passphrase) {
        (Some(_), Some(value)) => {
            if value.len() > 8192
                || value
                    .bytes()
                    .any(|byte| matches!(byte, 0..=8 | 10..=31 | 127))
            {
                return Err(SetupError("Invalid passphrase"));
            }
            Some((reference("passphrase"), value.into_bytes()))
        }
        _ => None,
    };
    let primary = Owned {
        credential: reference("credential"),
        signing: reference("signing"),
        public: key.into_bytes(),
        passphrase,
    };
    let mut identity = json!({ "credential": primary.credential.0, "signing": primary.signing.0 });
    if let Some((passphrase, _)) = &primary.passphrase {
        identity["passphrase"] = passphrase.0.clone().into();
    }
    let api_key_index_present = options
        .get("apiKeyIndex")
        .is_some_and(|value| !value.is_null() && value != "" && value != 0);
    for field in CREDENTIAL_FIELDS {
        options.remove(field);
    }
    if let Some(Value::Object(auth)) = options.get_mut("auth") {
        auth.remove("privateKey");
    }
    if secret_shaped(&options) {
        return Err(SetupError(
            "Engine options for a remote-signed account carry signing material",
        ));
    }
    options.insert("signing".into(), json!({ "primary": identity }));
    options.insert("remoteSigning".into(), true.into());
    options.insert("payloadSigner".into(), false.into());
    options.insert(
        "publicMode".into(),
        json!({
            "requested": requested,
            "remoteSigner": true,
            "payloadSigner": false,
            "apiKeyIndexPresent": api_key_index_present,
        }),
    );
    let inner = Arc::new(Inner::new(
        venue.to_owned(),
        policy,
        account_name,
        link,
        primary,
    ));
    Ok(RemoteAccount {
        provider: Arc::new(RemoteProvider {
            inner: inner.clone(),
        }),
        options_json: Value::Object(options).to_string(),
        inner,
    })
}

/// Any field still named like signing material after stripping, at any depth.
/// The stripped options are keyless by contract and may be logged.
fn secret_shaped(options: &Map<String, Value>) -> bool {
    options.iter().any(|(name, value)| {
        let name = name.to_ascii_lowercase().replace(['_', '-'], "");
        [
            "secret",
            "privatekey",
            "seed",
            "mnemonic",
            "passphrase",
            "agentkey",
            "starkkey",
            "l2key",
        ]
        .iter()
        .any(|shape| name.contains(shape))
            || nested_secret(value)
    })
}

fn nested_secret(value: &Value) -> bool {
    match value {
        Value::Object(inner) => secret_shaped(inner),
        Value::Array(items) => items.iter().any(nested_secret),
        _ => false,
    }
}

#[cfg(test)]
mod tests;
