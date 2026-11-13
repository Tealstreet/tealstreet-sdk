//! Public setup metadata and opaque facts produced by trusted local construction.
use crate::{EngineOptions, VenueError, invalid};

#[derive(Clone)]
pub struct TrustedSignerState {
    venue: String,
    public_key: Option<String>,
    #[cfg(feature = "lighter")]
    lighter_l1_address: Option<String>,
    #[cfg(feature = "paradex")]
    paradex_ethereum_key: bool,
}

impl std::fmt::Debug for TrustedSignerState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("TrustedSignerState")
            .field("venue", &self.venue)
            .finish_non_exhaustive()
    }
}

impl TrustedSignerState {
    /// Capture only after `prepare_local_signer_for` consumed private material.
    pub fn capture(venue: &str, options: &EngineOptions) -> Self {
        Self {
            venue: venue.into(),
            public_key: options.key.clone(),
            #[cfg(feature = "lighter")]
            lighter_l1_address: options.lighter_l1_address.clone(),
            #[cfg(feature = "paradex")]
            paradex_ethereum_key: options.paradex_ethereum_key,
        }
    }

    /// Only the public metadata crosses serialized account settings.
    pub fn public_options(&self) -> impl Iterator<Item = (&'static str, &str)> {
        let key = self.public_key.as_deref().map(|value| ("key", value));
        #[cfg(feature = "lighter")]
        let address = self
            .lighter_l1_address
            .as_deref()
            .map(|value| ("lighterL1Address", value));
        #[cfg(not(feature = "lighter"))]
        let address = None;
        key.into_iter().chain(address)
    }

    /// Opaque setup facts cannot be claimed through JSON or used on another venue.
    pub fn apply(&self, venue: &str, options: &mut EngineOptions) -> Result<(), VenueError> {
        if venue != self.venue {
            return Err(invalid("Trusted signer state belongs to another venue"));
        }
        options.key = self.public_key.clone();
        #[cfg(feature = "lighter")]
        {
            options.lighter_l1_address = self.lighter_l1_address.clone();
        }
        #[cfg(feature = "paradex")]
        {
            options.paradex_ethereum_key = self.paradex_ethereum_key;
        }
        Ok(())
    }
}

#[cfg(all(test, feature = "paradex", feature = "dispatch-signing"))]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn trusted_signer_state_preserves_derived_facts_and_refuses_another_venue() {
        let mut options: EngineOptions = serde_json::from_value(json!({
            "key": "1".repeat(64), "paradexAddress":"0x123", "subkeyAddress":"0x456"
        }))
        .unwrap();
        let (_, identities) = crate::prepare_local_signer_for("paradex", &mut options).unwrap();
        assert!(identities.primary.unwrap().signing_ref().is_ok());
        assert!(options.paradex_ethereum_key);
        let state = TrustedSignerState::capture("paradex", &options);
        let mut parsed: EngineOptions =
            serde_json::from_value(json!({"paradexEthereumKey":true})).unwrap();
        assert!(
            !parsed.paradex_ethereum_key,
            "JSON cannot assert a trusted derived-key fact"
        );
        assert!(state.apply("binance", &mut parsed).is_err());
        assert!(!parsed.paradex_ethereum_key);
        state.apply("paradex", &mut parsed).unwrap();
        assert!(parsed.paradex_ethereum_key);
        assert!(parsed.key.is_none() && parsed.secret.is_none());
    }
}
