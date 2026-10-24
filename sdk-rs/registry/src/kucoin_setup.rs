//! Trusted KuCoin broker input is consumed before opaque venue construction.
use serde::Deserialize;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct BrokerInput {
    pub partner: String,
    #[serde(default)]
    pub broker_key: String,
    pub broker_name: String,
    pub verify: Option<bool>,
}
safe_cex_crypto::redacted_debug!(BrokerInput);
impl Drop for BrokerInput {
    fn drop(&mut self) {
        use zeroize::Zeroize;
        self.broker_key.zeroize();
    }
}
impl BrokerInput {
    pub fn metadata(&self) -> safe_cex_venue_kucoin::sign::BrokerOptions {
        safe_cex_venue_kucoin::sign::BrokerOptions {
            partner: self.partner.clone(),
            broker_name: self.broker_name.clone(),
            verify: self.verify,
        }
    }
}

/// Shared construction has applied the declared setup policy before this boundary.
pub(crate) fn venue_options(
    options: &mut super::EngineOptions,
    provider_present: bool,
) -> Result<(super::KucoinOptions, safe_cex_engine::signing::Identities), safe_cex_engine::VenueError>
{
    use super::invalid;
    if [&options.key, &options.secret, &options.passphrase]
        .iter()
        .any(|value| value.as_ref().is_some_and(|value| !value.is_empty()))
        || options.read_credentials.is_some()
        || options
            .extra
            .as_ref()
            .and_then(|extra| extra.kucoin_broker.as_ref())
            .is_some_and(|broker| !broker.broker_key.is_empty())
    {
        return Err(invalid(
            "KuCoin credentials belong in signer setup, not engine options",
        ));
    }
    let mut identities = options.signing.take().unwrap_or_default();
    if let Some(read) = &identities.secondary {
        read.validate().map_err(invalid)?;
    }
    if identities
        .primary
        .as_ref()
        .is_some_and(|identity| identity.credential.0.is_empty() || identity.signing.0.is_empty())
    {
        identities.primary = None;
    }
    if let Some(primary) = &identities.primary {
        primary.validate().map_err(invalid)?;
    }
    if (identities.primary.is_some() || identities.secondary.is_some()) && !provider_present {
        return Err(invalid("Engine signing provider is required"));
    }
    let venue = super::KucoinOptions {
        credentials: identities.primary.clone(),
        read_credentials: identities.secondary.clone(),
        broker: options
            .extra
            .take()
            .and_then(|extra| extra.kucoin_broker)
            .map(|broker| broker.metadata()),
        testnet: options.testnet,
        public_data_only: options.public_data_only,
    };
    Ok((venue, identities))
}
