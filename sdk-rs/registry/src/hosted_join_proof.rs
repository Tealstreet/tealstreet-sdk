//! Closed device admission recipe; no account activation or venue transactions.
use safe_cex_crypto::admission::{
    Family,
    identity::Venue,
    wire::{Request, Response},
};
use safe_cex_engine::signing::{
    Identities, KeyRef, SignError, SignReceipt, SignRequest, SignResult,
};

const MAX_AGE_MS: u64 = 15_000;
const MAX_SAFE_INTEGER: u64 = 9_007_199_254_740_991;

pub struct Account {
    venue: Venue,
    name: String,
    family: Family,
    key: KeyRef,
}

pub struct Pending {
    id: String,
    family: Family,
}

impl Account {
    pub fn capture(
        venue: &str,
        identities: &Identities,
        options: &crate::EngineOptions,
    ) -> Result<Self, SignError> {
        let _ = options;
        let supported = match venue {
            "backpack" => cfg!(feature = "backpack"),
            "hyperliquid" | "hyperliquidspot" => cfg!(feature = "hyperliquid"),
            "grvt" => cfg!(feature = "grvt"),
            "txflow" => cfg!(feature = "txflow"),
            "polymarket" => cfg!(feature = "polymarket"),
            "lighter" | "lighter-robinhood" => cfg!(feature = "lighter"),
            "paradex" => cfg!(feature = "paradex"),
            "orderly" | "kodiakfi" | "mode" | "what" | "woofi" => cfg!(feature = "orderly"),
            #[cfg(feature = "asterdex")]
            "asterdex" => options
                .asterdex
                .api_version
                .is_some_and(|version| version.prefix() == "/fapi/v3"),
            _ => false,
        };
        if !supported {
            return Err(SignError::Unsupported);
        }
        let family = if venue == "asterdex" {
            Family::Ethereum
        } else {
            Family::for_venue(venue).ok_or(SignError::Unsupported)?
        };
        let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
        identity.validate().map_err(|_| SignError::InvalidRequest)?;
        Ok(Self {
            venue: serde_json::from_value(venue.into()).map_err(|_| SignError::Unsupported)?,
            name: venue.into(),
            family,
            key: identity.signing_ref()?.clone(),
        })
    }

    pub fn needs_entropy(&self) -> bool {
        self.family == Family::Schnorr
    }

    pub fn prepare(
        &self,
        raw: &str,
        now_ms: u64,
        entropy: &[u8; 32],
    ) -> Result<(SignRequest, Pending), SignError> {
        let request = Request::from_json(raw)?;
        if request.venue() != self.venue || !request.statement().current_at(now_ms, MAX_AGE_MS) {
            return Err(SignError::InvalidRequest);
        }
        let issued = request.statement().issued_at_ms();
        let expiry = issued
            .checked_add(MAX_AGE_MS)
            .filter(|expiry| *expiry <= MAX_SAFE_INTEGER)
            .ok_or(SignError::InvalidRequest)?;
        let mut work = request
            .statement()
            .sign_request(self.family, &self.key, entropy)?;
        work.context.venue = self.name.clone();
        work.context.method = "JOIN-PROOF".into();
        work.receipt = SignReceipt {
            stamp_ms: Some(issued as f64),
            expiry_ms: Some(expiry as f64),
            nonce: None,
        };
        work.validate()?;
        Ok((
            work,
            Pending {
                id: request.id().into(),
                family: self.family,
            },
        ))
    }
}

impl Pending {
    /// The existing Capture validates the original work/receipt before formatting.
    pub fn finish(self, result: &SignResult) -> Result<Response, SignError> {
        if !result.headers.is_empty() || result.values.len() != 1 {
            return Err(SignError::InvalidResult);
        }
        Response::signed(&self.id, self.family, &result.values[0])
    }
}
