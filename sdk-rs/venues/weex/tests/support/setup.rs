//! Raw test inputs enter the trusted local provider under the WEEX setup policy;
//! the venue only ever receives opaque identities.
#![allow(dead_code)]
use super::{NativeOptions, WeexFutures};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{
    CredentialSetupPolicy, IdentitySetupPolicy, Provider, SetupFieldPolicy, SignError,
};
use safe_cex_engine::{Engine, ReadCredentials};

#[derive(Clone, Default)]
pub struct Options {
    pub read_credentials: Option<ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub stable_id: Option<String>,
    pub private_ws_headers: bool,
}

/// The declared WEEX policy: primary passphrase required for private use, an
/// absent secondary passphrase inherits the primary ref, explicit empty rejects.
pub fn policy() -> CredentialSetupPolicy {
    CredentialSetupPolicy {
        primary: Some(IdentitySetupPolicy {
            passphrase: Some(SetupFieldPolicy::RequiredForPrivate),
            ..Default::default()
        }),
        secondary: Some(IdentitySetupPolicy {
            passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
            ..Default::default()
        }),
    }
}

impl Options {
    /// Trusted setup; incomplete material is refused here, before construction.
    pub fn try_build(mut self) -> Result<(WeexFutures, Provider), SignError> {
        let supplied = |value: &Option<String>| value.as_deref().is_some_and(|s| !s.is_empty());
        let primary = (supplied(&self.key) || supplied(&self.secret) || supplied(&self.passphrase))
            .then(|| LocalIdentity {
                key: Secret::new(self.key.take().unwrap_or_default()),
                secret: Secret::new(self.secret.take().unwrap_or_default()),
                passphrase: self.passphrase.take().map(Secret::new),
                stable_id: self.stable_id.take(),
                ..Default::default()
            });
        let secondary = self.read_credentials.take().map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        });
        let (local, identities) = LocalSigner::from_setup_with_policy_and_mode(
            LocalSetup { primary, secondary },
            policy(),
            self.public_data_only,
        )?;
        Ok((
            WeexFutures::new(NativeOptions {
                credentials: identities.primary,
                read_credentials: identities.secondary,
                public_data_only: self.public_data_only,
                private_ws_headers: self.private_ws_headers,
            }),
            std::sync::Arc::new(local),
        ))
    }
    pub fn build(self) -> (WeexFutures, Provider) {
        self.try_build().expect("trusted WEEX test setup")
    }
}

pub fn engine(options: Options) -> Engine<WeexFutures> {
    with_venue(options, |venue| venue)
}

/// Install a wrapper around the trusted provider, e.g. to refuse chosen work.
pub fn engine_with_provider(
    options: Options,
    wrap: impl FnOnce(Provider) -> Provider,
) -> Engine<WeexFutures> {
    let (venue, local) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(wrap(local))).unwrap();
    engine
}

/// Signs or refuses each request in `plan` order, then signs everything after.
pub struct Gated {
    pub inner: Provider,
    pub plan: std::sync::Mutex<std::collections::VecDeque<bool>>,
}
impl Gated {
    pub fn new(inner: Provider, plan: &[bool]) -> Self {
        Self {
            inner,
            plan: std::sync::Mutex::new(plan.iter().copied().collect()),
        }
    }
    fn admit(&self) -> bool {
        self.plan.lock().unwrap().pop_front().unwrap_or(true)
    }
}
impl safe_cex_engine::signing::Signer for Gated {
    fn immediate(&self) -> bool {
        true
    }
    fn sign(
        &self,
        request: safe_cex_engine::signing::SignRequest,
    ) -> safe_cex_engine::signing::SignFuture {
        self.inner.sign(request)
    }
    fn sign_ready(
        &self,
        request: &safe_cex_engine::signing::SignRequest,
    ) -> Option<Result<safe_cex_engine::signing::SignResult, SignError>> {
        if self.admit() {
            self.inner.sign_ready(request)
        } else {
            Some(Err(SignError::Refused))
        }
    }
}

/// Wrap the trusted venue (for probes) and install its provider.
pub fn with_venue<V: safe_cex_engine::Venue>(
    options: Options,
    wrap: impl FnOnce(WeexFutures) -> V,
) -> Engine<V> {
    let (venue, local) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(wrap(venue));
    engine.set_signer(Some(local)).unwrap();
    engine
}

/// Independent native oracle: literal secret bytes, Base64 raw MAC, native order.
pub fn oracle(
    key: &str,
    secret: &str,
    passphrase: &str,
    method: safe_cex_engine::Method,
    path_query: &str,
    body: &str,
    stamp: &str,
) -> Vec<(&'static str, String)> {
    let payload = format!("{stamp}{}{path_query}{body}", method.as_str());
    vec![
        ("ACCESS-KEY", key.into()),
        ("ACCESS-PASSPHRASE", passphrase.into()),
        ("ACCESS-TIMESTAMP", stamp.into()),
        (
            "ACCESS-SIGN",
            safe_cex_crypto::hmac_sha256(
                secret.as_bytes(),
                payload.as_bytes(),
                safe_cex_crypto::Encoding::Base64,
            ),
        ),
        ("Content-Type", "application/json".into()),
        ("User-Agent", "Tealstreet".into()),
    ]
}
