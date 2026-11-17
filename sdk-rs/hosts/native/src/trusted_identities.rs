//! Provider-bound identity facts captured by trusted local host construction.
use safe_cex_engine::signing::{Identities, Provider};
use std::{io, sync::Arc};

/// Keeps private setup facts in process; never construct this from account JSON.
#[derive(Clone)]
pub struct TrustedSignerIdentities {
    venue: String,
    provider: Provider,
    identities: Identities,
}

impl std::fmt::Debug for TrustedSignerIdentities {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("TrustedSignerIdentities")
    }
}

impl TrustedSignerIdentities {
    /// Capture the provider and identities returned together by trusted local setup.
    pub fn capture(venue: &str, provider: Provider, identities: Identities) -> Self {
        Self {
            venue: venue.into(),
            provider,
            identities,
        }
    }

    pub(crate) fn apply(
        &self,
        venue: &str,
        provider: Option<&Provider>,
        options: &mut safe_cex_venues::EngineOptions,
    ) -> io::Result<()> {
        let invalid = || {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "Trusted signer identity mismatch",
            )
        };
        if venue != self.venue || !provider.is_some_and(|p| Arc::ptr_eq(p, &self.provider)) {
            return Err(invalid());
        }
        let supplied = serde_json::to_value(&options.signing).map_err(|_| invalid())?;
        let captured = serde_json::to_value(Some(&self.identities)).map_err(|_| invalid())?;
        if supplied != captured {
            return Err(invalid());
        }
        // Serialization deliberately drops ScalarOnly absence and other private facts.
        // Restore only the original refs on the exact provider and venue that own them.
        options.signing = Some(self.identities.clone());
        Ok(())
    }
}
