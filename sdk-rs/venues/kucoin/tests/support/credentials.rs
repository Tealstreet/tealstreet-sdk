//! Raw material belongs only to this native compatibility fixture and its provider.
pub use super::native_sign::BrokerOptions;
use safe_cex_crypto::signer::{LocalExtra, LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::ReadCredentials;
use safe_cex_engine::signing::{
    CredentialSetupPolicy, ExtraFieldPolicy, ExtraKey, ExtraKeyPolicy, Identities,
    IdentitySetupPolicy, KeyEncoding, SetupFieldPolicy,
};

#[derive(Clone, Default)]
pub struct Options {
    pub key: Option<String>,
    pub secret: Option<String>,
    pub passphrase: Option<String>,
    pub read_credentials: Option<ReadCredentials>,
    pub testnet: bool,
    pub public_data_only: bool,
    pub broker: Option<BrokerOptions>,
}
safe_cex_crypto::redacted_debug!(Options);
impl safe_cex_crypto::secrets::EraseSecrets for Options {
    fn erase_secrets(&mut self) {
        safe_cex_crypto::secrets::erase(&mut self.key);
        safe_cex_crypto::secrets::erase(&mut self.secret);
        safe_cex_crypto::secrets::erase(&mut self.passphrase);
        if let Some(read) = &mut self.read_credentials {
            read.erase_secrets();
        }
        if let Some(broker) = &mut self.broker {
            broker.erase_secrets();
        }
    }
}
impl Options {
    pub fn prepare(self) -> (safe_cex_venue_kucoin::Options, LocalSigner, Identities) {
        let mut primary = self
            .key
            .zip(self.secret)
            .map(|(key, secret)| LocalIdentity {
                key: Secret::new(key),
                secret: Secret::new(secret),
                passphrase: self.passphrase.map(Secret::new),
                ..Default::default()
            });
        if let (Some(identity), Some(broker)) = (&mut primary, &self.broker) {
            identity.extra.0.insert(
                ExtraKey::KucoinBroker,
                LocalExtra {
                    value: Secret::new(broker.broker_key.clone()),
                    encoding: KeyEncoding::Utf8,
                },
            );
        }
        let secondary = self.read_credentials.map(|read| LocalIdentity {
            key: Secret::new(read.key),
            secret: Secret::new(read.secret),
            passphrase: read.passphrase.map(Secret::new),
            ..Default::default()
        });
        let policy = CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                passphrase: Some(SetupFieldPolicy::RequiredForPrivate),
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::KucoinBroker,
                    field: ExtraFieldPolicy::Optional,
                    encoding: Some(KeyEncoding::Utf8),
                    trim: true,
                }),
                ..Default::default()
            }),
            secondary: Some(IdentitySetupPolicy {
                passphrase: Some(SetupFieldPolicy::RequiredWithPrimaryFallback),
                extra: Some(ExtraKeyPolicy {
                    key: ExtraKey::KucoinBroker,
                    field: ExtraFieldPolicy::AliasPrimary,
                    encoding: None,
                    trim: false,
                }),
                ..Default::default()
            }),
        };
        let (provider, identities) = LocalSigner::from_setup_with_policy_and_mode(
            LocalSetup { primary, secondary },
            policy,
            self.public_data_only,
        )
        .expect("valid compatibility fixture setup");
        let options = safe_cex_venue_kucoin::Options {
            credentials: identities.primary.clone(),
            read_credentials: identities.secondary.clone(),
            testnet: self.testnet,
            public_data_only: self.public_data_only,
            broker: self
                .broker
                .map(|broker| safe_cex_venue_kucoin::sign::BrokerOptions {
                    partner: broker.partner.clone(),
                    broker_name: broker.broker_name.clone(),
                    verify: broker.verify,
                }),
        };
        (options, provider, identities)
    }
}

pub fn engine<V: safe_cex_engine::Venue>(
    options: Options,
    constructor: impl FnOnce(safe_cex_venue_kucoin::Options) -> V,
) -> safe_cex_engine::Engine<V> {
    with_seed(options, constructor, 17)
}

pub fn with_seed<V: safe_cex_engine::Venue>(
    options: Options,
    constructor: impl FnOnce(safe_cex_venue_kucoin::Options) -> V,
    seed: u64,
) -> safe_cex_engine::Engine<V> {
    let (options, provider, identities) = options.prepare();
    let mut engine = safe_cex_engine::Engine::new(
        constructor(options),
        Some(safe_cex_engine::test_support::seeded_entropy(seed)),
    )
    .unwrap();
    engine
        .set_signer(Some(std::sync::Arc::new(provider)))
        .unwrap();
    engine.set_identities(identities).unwrap();
    engine
}
