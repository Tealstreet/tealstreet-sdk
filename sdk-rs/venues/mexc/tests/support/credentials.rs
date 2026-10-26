//! Trusted fixtures consume legacy inputs before constructing the real venue.
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SigningPolicy};
use safe_cex_engine::{Engine, ReadCredentials};
use safe_cex_venue_mexc::MexcFutures;
use std::sync::Arc;
#[derive(Default)]
pub struct Options {
    pub recv_window: Option<u32>,
    pub read_credentials: Option<ReadCredentials>,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
}
pub fn configured(mut options: Options) -> Engine<MexcFutures> {
    let raw = options.key.take();
    let primary = raw.as_ref().filter(|key| !key.is_empty()).map(|key| {
        let hash = safe_cex_crypto::sha256(key.as_bytes());
        LocalIdentity {
            key: Secret::new(key.clone()),
            secret: Secret::new(options.secret.take().unwrap_or_default()),
            stable_id: Some(format!(
                "mexc:{}",
                safe_cex_crypto::encode(&hash[..12], safe_cex_crypto::Encoding::Hex)
            )),
            ..Default::default()
        }
    });
    let valid = options
        .read_credentials
        .as_ref()
        .map(|read| read.validate().is_ok());
    let secondary = options
        .read_credentials
        .take()
        .filter(|_| valid == Some(true))
        .map(|mut read| LocalIdentity {
            key: Secret::new(std::mem::take(&mut read.key)),
            secret: Secret::new(std::mem::take(&mut read.secret)),
            ..Default::default()
        });
    let (provider, identities) = LocalSigner::from_setup_with_policy(
        LocalSetup { primary, secondary },
        CredentialSetupPolicy {
            primary: Some(IdentitySetupPolicy {
                trim_credential: Some(true),
                signing: SigningPolicy::Optional,
                ..Default::default()
            }),
            secondary: None,
        },
    )
    .unwrap();
    let mut engine = safe_cex_engine::test_support::seeded_engine(MexcFutures::new(
        safe_cex_venue_mexc::Options {
            recv_window: options.recv_window,
            public_data_only: options.public_data_only,
            identities: identities.clone(),
            read_credentials_valid: valid,
        },
    ));
    engine.set_signer(Some(Arc::new(provider))).unwrap();
    engine.set_identities(identities).unwrap();
    engine
}
