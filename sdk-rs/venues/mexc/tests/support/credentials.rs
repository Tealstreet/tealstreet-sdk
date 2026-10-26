//! Trusted fixtures consume legacy inputs before constructing the real venue.
use super::TestEngine;
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::ReadCredentials;
use safe_cex_engine::signing::{CredentialSetupPolicy, IdentitySetupPolicy, SigningPolicy};
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
pub fn configured(options: Options) -> TestEngine {
    configured_with_provider(options, |provider| provider)
}
pub fn configured_with_provider(
    mut options: Options,
    wrap: impl FnOnce(safe_cex_engine::signing::Provider) -> safe_cex_engine::signing::Provider,
) -> TestEngine {
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
    let provider = wrap(Arc::new(provider));
    engine.set_signer(Some(provider.clone())).unwrap();
    engine.set_identities(identities).unwrap();
    engine.into()
}
