#![allow(dead_code)]
pub mod trading;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{Identities, Identity, KeyRef, Provider};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_bitrue::{Bitrue, Options as NativeOptions};
#[derive(Clone)]
pub struct Options {
    pub read_credentials: Option<ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub recv_window: u32,
}
impl Default for Options {
    fn default() -> Self {
        Self {
            read_credentials: None,
            public_data_only: false,
            key: None,
            secret: None,
            recv_window: 20_000,
        }
    }
}
impl Options {
    pub fn native(&self, identities: &Identities) -> NativeOptions {
        let read_credentials = if self
            .read_credentials
            .as_ref()
            .is_some_and(|r| r.validate().is_err())
        {
            Some(Identity::new(
                KeyRef(String::new()),
                Some(KeyRef(String::new())),
            ))
        } else {
            identities.secondary.clone()
        };
        NativeOptions {
            credentials: identities.primary.clone(),
            read_credentials,
            public_data_only: self.public_data_only,
            recv_window: self.recv_window,
        }
    }
    pub fn build(self) -> (Bitrue, Provider, Identities) {
        let primary = self
            .key
            .as_ref()
            .zip(self.secret.as_ref())
            .filter(|(k, s)| !k.is_empty() && !s.is_empty())
            .map(|(k, s)| LocalIdentity {
                key: Secret::new(k.clone()),
                secret: Secret::new(s.clone()),
                ..Default::default()
            });
        let secondary = self
            .read_credentials
            .as_ref()
            .filter(|r| r.validate().is_ok())
            .map(|r| LocalIdentity {
                key: Secret::new(r.key.clone()),
                secret: Secret::new(r.secret.clone()),
                ..Default::default()
            });
        let (provider, ids) = LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        (
            Bitrue::new(self.native(&ids)),
            std::sync::Arc::new(provider),
            ids,
        )
    }
}
pub fn engine(options: Options) -> Engine<Bitrue> {
    let (venue, provider, identities) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(provider)).unwrap();
    engine.set_identities(identities).unwrap();
    engine
}

safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        if let Some(read) = self.read_credentials.as_mut() {
            read.erase_secrets();
        }
    }
}
