//! Trusted native fixture setup. Raw fixtures are consumed before venue construction.
#![allow(dead_code)]
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_bybit::{BybitLinear, Endpoints, Options as VenueOptions, Variant};
#[derive(Clone, Default)]
pub struct Options {
    pub signing_policy: safe_cex_engine::signing_policy::RemoteSigningPolicy,
    pub recv_window: Option<u32>,
    pub read_credentials: Option<ReadCredentials>,
    pub variant: Variant,
    pub endpoints: Option<Endpoints>,
    pub testnet: bool,
    pub bybit_site_id: Option<String>,
    pub remote_signing: bool,
    pub enable_inverse_markets: bool,
    pub enable_option_markets: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
impl Options {
    pub fn validate(&self) -> Result<(), String> {
        VenueOptions {
            variant: self.variant,
            endpoints: self.endpoints.clone(),
            ..VenueOptions::default()
        }
        .validate()
    }

    pub fn build(mut self) -> (BybitLinear, safe_cex_engine::signing::Provider) {
        let primary = if self.key.is_some() || self.secret.is_some() {
            Some(LocalIdentity {
                key: Secret::new(self.key.take().unwrap_or_default()),
                secret: Secret::new(self.secret.take().unwrap_or_default()),
                ..Default::default()
            })
        } else {
            None
        };
        let secondary = self.read_credentials.take().map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            passphrase: read.passphrase.take().map(Secret::new),
            ..Default::default()
        });
        let (signer, identities) =
            LocalSigner::from_setup(LocalSetup { primary, secondary }).unwrap();
        let venue = BybitLinear::new(VenueOptions {
            signing_policy: self.signing_policy,
            credentials: identities.primary,
            read_credentials: identities.secondary,
            recv_window: self.recv_window,
            variant: self.variant,
            endpoints: self.endpoints,
            testnet: self.testnet,
            bybit_site_id: self.bybit_site_id,
            remote_signing: self.remote_signing,
            enable_inverse_markets: self.enable_inverse_markets,
            enable_option_markets: self.enable_option_markets,
            public_data_only: self.public_data_only,
        });
        (venue, std::sync::Arc::new(signer))
    }
}
pub fn engine(options: Options) -> Engine<BybitLinear> {
    let (venue, signer) = options.build();
    let mut engine = safe_cex_engine::test_support::seeded_engine(venue);
    engine.set_signer(Some(signer)).unwrap();
    engine
}
