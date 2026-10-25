#![allow(dead_code)]
use safe_cex_crypto::secrets::EraseSecrets;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_phemex::{Options as NativeOptions, PhemexPerpetuals};
#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub read_credentials: Option<ReadCredentials>,
}
impl std::fmt::Debug for Options {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustedFixtureOptions")
            .finish_non_exhaustive()
    }
}
impl EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = &mut self.read_credentials {
            read.erase_secrets();
        }
    }
}
impl Options {
    pub fn build(mut self) -> (PhemexPerpetuals, safe_cex_engine::signing::Provider) {
        let primary = self
            .key
            .take()
            .zip(self.secret.take())
            .filter(|(k, s)| !k.is_empty() && !s.is_empty())
            .map(|(key, secret)| LocalIdentity {
                key: Secret::new(key),
                secret: Secret::new(secret),
                ..Default::default()
            });
        let invalid_secondary = self
            .read_credentials
            .as_ref()
            .is_some_and(|read| read.key.is_empty() || read.secret.is_empty());
        let secondary = self
            .read_credentials
            .take()
            .filter(|_| !invalid_secondary)
            .map(|mut read| LocalIdentity {
                key: Secret::new(std::mem::take(&mut read.key)),
                secret: Secret::new(std::mem::take(&mut read.secret)),
                passphrase: read.passphrase.take().map(Secret::new),
                ..Default::default()
            });
        let (local, mut identities) =
            LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        if invalid_secondary {
            identities.secondary = Some(safe_cex_engine::signing::Identity {
                credential: safe_cex_engine::signing::KeyRef(String::new()),
                signing: safe_cex_engine::signing::KeyRef(String::new()),
                passphrase: None,
                stable_id: None,
                extra: Default::default(),
            });
        }
        (
            PhemexPerpetuals::new(NativeOptions {
                testnet: self.testnet,
                public_data_only: self.public_data_only,
                credentials: identities.primary,
                read_credentials: identities.secondary,
            }),
            std::sync::Arc::new(local),
        )
    }
}
pub fn engine(options: Options) -> Engine<PhemexPerpetuals> {
    let (venue, provider) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(provider)).unwrap();
    engine
}

/// Immediate native fixtures use real routes at their most recent ingress time.
/// Managed-boundary tests keep the raw Engine and control each host transition.
pub struct PreparedEngine<V: safe_cex_engine::Venue> {
    inner: Engine<V>,
    now: f64,
}
impl<V: safe_cex_engine::Venue> std::ops::Deref for PreparedEngine<V> {
    type Target = Engine<V>;
    fn deref(&self) -> &Self::Target {
        &self.inner
    }
}
impl<V: safe_cex_engine::Venue> std::ops::DerefMut for PreparedEngine<V> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.inner
    }
}
impl<V: safe_cex_engine::Venue> PreparedEngine<V> {
    pub fn start(&mut self, now: f64) {
        self.now = now;
        self.inner.start(now);
    }
    pub fn handle(&mut self, now: f64, event: safe_cex_engine::Event<'_>) {
        self.now = now;
        self.inner.handle(now, event);
    }
    pub fn drain_raw_actions(&mut self) -> Vec<safe_cex_engine::Action> {
        self.inner.prepare_dispatch(self.now);
        self.inner.drain_actions().collect()
    }
    pub fn drain_actions(&mut self) -> std::vec::IntoIter<safe_cex_engine::Action> {
        safe_cex_engine::test_support::prepared_actions(&mut self.inner, self.now).into_iter()
    }
}
pub fn prepared_engine(options: Options) -> PreparedEngine<PhemexPerpetuals> {
    PreparedEngine {
        inner: engine(options),
        now: 0.,
    }
}
