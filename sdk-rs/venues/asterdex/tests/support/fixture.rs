//! Test-only trusted setup with the Aster mapping: raw fixture credentials go
//! to a local provider, and the venue receives refs and public config only.
//! Shared with the crate's unit tests through `#[path]`.
use super::sign::{Version, resolve_version};
use super::{AsterDex, VenueOptions};
use safe_cex_crypto::signer::{LocalIdentity, LocalSetup, LocalSigner, Secret};
use safe_cex_engine::signing::KeyEncoding;

#[derive(Clone, Default)]
pub struct Options {
    pub testnet: bool,
    pub public_data_only: bool,
    pub key: Option<String>,
    pub secret: Option<String>,
    pub application_id: Option<String>,
    pub auth_user_address: Option<String>,
    pub auth_signer_address: Option<String>,
    pub auth_private_key: Option<String>,
    pub api_version: Option<Version>,
    /// Caller-assigned primary account identity.
    pub stable_id: Option<String>,
    pub read_key: Option<String>,
    pub read_secret: Option<String>,
    pub builder_address: Option<String>,
    pub builder_fee_rate_maker: Option<String>,
    pub builder_fee_rate_taker: Option<String>,
}

fn nonempty(value: &Option<String>) -> Option<&str> {
    value.as_deref().map(str::trim).filter(|v| !v.is_empty())
}

impl Options {
    /// A V3 API-wallet account on public test material.
    pub fn v3_wallet() -> Self {
        Self {
            key: Some("0x21cF8Ae13Bb72632562c6Fff438652Ba1a151bb0".into()),
            secret: Some("4fd0a42218f3eae43a6ce26d22544e986139a01e5b34a62db53757ffca81bae1".into()),
            application_id: Some("0x63DD5aCC6b1aa0f563956C0e534DD30B6dcF7C4e".into()),
            ..Self::default()
        }
    }
    /// A V1 HMAC account with its own read agent, on fixture material.
    pub fn v1_with_read_agent() -> Self {
        Self {
            key: Some("primary-key".into()),
            secret: Some("primary-secret".into()),
            read_key: Some("read-key".into()),
            read_secret: Some("read-secret".into()),
            ..Self::default()
        }
    }
    pub fn build(self) -> (AsterDex, safe_cex_engine::signing::Provider) {
        self.try_build().expect("valid fixture setup")
    }

    /// Setup refuses material the resolved version cannot sign with.
    pub fn try_build(
        self,
    ) -> Result<(AsterDex, safe_cex_engine::signing::Provider), safe_cex_engine::signing::SignError>
    {
        let user = nonempty(&self.auth_user_address).or(nonempty(&self.application_id));
        let signer = nonempty(&self.auth_signer_address).or(nonempty(&self.key));
        let wallet = nonempty(&self.auth_private_key).or(nonempty(&self.secret));
        let version = resolve_version(self.api_version, user, signer, wallet);
        let encoding = match version {
            Version::V1 => KeyEncoding::Utf8,
            Version::V3 => KeyEncoding::Hex,
        };
        let primary_secret = match version {
            Version::V1 => self.secret.clone(),
            Version::V3 => wallet.map(str::to_owned),
        };
        let primary = (self.key.is_some() || primary_secret.is_some()).then(|| LocalIdentity {
            key: Secret::new(self.key.clone().unwrap_or_default()),
            secret: Secret::new(primary_secret.unwrap_or_default()),
            secret_encoding: encoding,
            stable_id: self.stable_id.clone(),
            ..Default::default()
        });
        let alias = version == Version::V3 && nonempty(&self.auth_private_key).is_some();
        let secondary =
            self.read_key
                .as_ref()
                .zip(self.read_secret.as_ref())
                .map(|(key, secret)| LocalIdentity {
                    key: Secret::new(key.clone()),
                    secret: Secret::new(if alias { String::new() } else { secret.clone() }),
                    secret_encoding: encoding,
                    signing_from_primary: alias,
                    ..Default::default()
                });
        let (provider, identities) = LocalSigner::from_setup(LocalSetup { primary, secondary })?;
        let venue = AsterDex::new(VenueOptions {
            testnet: self.testnet,
            public_data_only: self.public_data_only,
            credentials: identities.primary,
            read_credentials: identities.secondary,
            api_version: Some(version),
            user_address: user.map(str::to_owned),
            signer_address: signer.map(str::to_owned),
            read_signer_address: nonempty(&self.auth_signer_address)
                .or(nonempty(&self.read_key))
                .map(str::to_owned),
            builder_address: self.builder_address,
            builder_fee_rate_maker: self.builder_fee_rate_maker,
            builder_fee_rate_taker: self.builder_fee_rate_taker,
        });
        Ok((venue, std::sync::Arc::new(provider)))
    }
}
