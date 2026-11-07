//! Trusted broker material is consumed before opaque venue construction.
use safe_cex_crypto::signer::{LocalExtra, LocalIdentity, Secret};
use safe_cex_engine::signing::{ExtraKey, Identities, KeyEncoding};
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

pub(super) fn take_broker(options: &mut super::EngineOptions, primary: Option<&mut LocalIdentity>) {
    let Some(broker) = options
        .extra
        .as_mut()
        .and_then(|extra| extra.kucoin_broker.as_mut())
    else {
        return;
    };
    let material = Secret::new(std::mem::take(&mut broker.broker_key));
    if let Some(primary) = primary
        && !broker.partner.trim().is_empty()
        && !broker.broker_name.trim().is_empty()
    {
        primary.extra.0.insert(
            ExtraKey::KucoinBroker,
            LocalExtra {
                value: material,
                encoding: KeyEncoding::Utf8,
            },
        );
    }
}

pub(super) fn venue_options(
    options: &mut super::EngineOptions,
    provider: bool,
) -> Result<(super::KucoinOptions, Identities), super::VenueError> {
    if [&options.key, &options.secret, &options.passphrase]
        .iter()
        .any(|v| v.is_some())
        || options.read_credentials.is_some()
        || options
            .extra
            .as_ref()
            .and_then(|e| e.kucoin_broker.as_ref())
            .is_some_and(|b| !b.broker_key.is_empty())
    {
        return Err(super::invalid(
            "KuCoin credentials belong in signer setup, not engine options",
        ));
    }
    let mut identities = options.signing.take().unwrap_or_default();
    if let Some(read) = &identities.secondary {
        read.validate().map_err(super::invalid)?;
    }
    if identities
        .primary
        .as_ref()
        .is_some_and(|id| id.validate().is_err())
    {
        identities.primary = None;
    }
    if (identities.primary.is_some() || identities.secondary.is_some()) && !provider {
        return Err(super::invalid("Engine signing provider is required"));
    }
    let venue = super::KucoinOptions {
        credentials: identities.primary.clone(),
        read_credentials: identities.secondary.clone(),
        testnet: options.testnet,
        public_data_only: options.public_data_only,
        broker: options.extra.take().and_then(|e| e.kucoin_broker).map(|b| {
            safe_cex_venue_kucoin::sign::BrokerOptions {
                partner: b.partner.clone(),
                broker_name: b.broker_name.clone(),
                verify: b.verify,
            }
        }),
    };
    Ok((venue, identities))
}
