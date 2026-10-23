//! Engine-owned monotonic signing nonces, shared by every engine in one native
//! process or Wasm module. Separate processes / Wasm memories must coordinate
//! at their deployment boundary. See docs/plans/RUST_CREDENTIAL_SELECTION.md (C3).
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

type NonceMap = HashMap<(&'static str, [u8; 32]), u64>;
static NONCES: OnceLock<Mutex<NonceMap>> = OnceLock::new();

/// A nonce domain's signer identity. Built only from non-secret material, so
/// no key digest or slot name can become a nonce domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonceKey([u8; 32]);

impl NonceKey {
    /// Trusted, caller-assigned `Identity.stable_id`; tagged apart from public ids.
    pub fn stable_id(id: &str) -> Self {
        let mut hash = Sha256::new();
        hash.update(b"safe-cex stable id\0");
        hash.update(id.as_bytes());
        Self(hash.finalize().into())
    }

    /// A public Ethereum address; hex case and `0x` prefix select the same key.
    pub fn ethereum_address(address: &str) -> Self {
        let lower = address.to_ascii_lowercase();
        let normalized = if lower.starts_with("0x") {
            lower
        } else {
            format!("0x{lower}")
        };
        Self(Sha256::digest(normalized.as_bytes()).into())
    }
}

/// Clock floor for a reservation, in the family's wire unit.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NonceFloor {
    /// Raw host milliseconds, floored, times `scale`.
    RawMs { scale: u64 },
    /// Calibrated protocol milliseconds, floored, times `scale`.
    ProtocolMs { scale: u64 },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum NonceError {
    InvalidTime,
    Exhausted,
    Unavailable,
}

impl NonceError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidTime => "Invalid signing time",
            Self::Exhausted => "Signing nonce exhausted",
            Self::Unavailable => "Signing nonce state unavailable",
        }
    }
}

/// `max(floor, previous + 1)`; state survives signers dropping and engines restarting.
pub(crate) fn reserve(
    domain: &'static str,
    key: [u8; 32],
    floor: u64,
    limit: u64,
) -> Result<u64, NonceError> {
    let mut nonces = NONCES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| NonceError::Unavailable)?;
    let previous = nonces.entry((domain, key)).or_default();
    let nonce = floor.max(previous.checked_add(1).ok_or(NonceError::Exhausted)?);
    if nonce > limit {
        return Err(NonceError::Exhausted);
    }
    *previous = nonce;
    Ok(nonce)
}

pub(crate) fn floor(ms: f64, scale: u64, limit: u64) -> Result<u64, NonceError> {
    let scaled = ms.floor() * scale as f64;
    if !ms.is_finite() || ms < 0. || !(0.0..=limit as f64).contains(&scaled) {
        return Err(NonceError::InvalidTime);
    }
    Ok(scaled as u64)
}

impl<P> crate::Cx<'_, P> {
    /// Reserve a signing nonce before signing, in one shared monotonic domain.
    pub fn reserve_nonce(
        &self,
        domain: &'static str,
        key: &NonceKey,
        floor: NonceFloor,
        limit: u64,
    ) -> Result<u64, NonceError> {
        let (ms, scale) = match floor {
            NonceFloor::RawMs { scale } => (self.now, scale),
            NonceFloor::ProtocolMs { scale } => (self.protocol_now(), scale),
        };
        reserve(domain, key.0, self::floor(ms, scale, limit)?, limit)
    }
}

/// Kraken's secret-digest domain until its signer migration moves it to
/// `NonceKey::stable_id`. Frozen by check-credential-selection.py.
#[doc(hidden)]
pub fn reserve_legacy_digest(
    domain: &'static str,
    identity: [u8; 32],
    floor: u64,
    limit: u64,
) -> Result<u64, String> {
    reserve(domain, identity, floor, limit).map_err(|error| error.message().into())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn instances_and_restarts_share_a_monotonic_sequence_despite_clock_rollback() {
        let id = NonceKey::stable_id("restart test signer").0;
        assert_eq!(reserve("test", id, 100, 1000), Ok(100));
        assert_eq!(reserve("test", id, 100, 1000), Ok(101));
        assert_eq!(reserve("test", id, 0, 1000), Ok(102));
        let other = NonceKey::stable_id("other signer").0;
        assert_eq!(reserve("test", other, 100, 1000), Ok(100));
        assert_eq!(reserve("test", id, 1001, 1000), Err(NonceError::Exhausted));
        assert_eq!(reserve("test", id, 1, 1000), Ok(103));
    }

    #[test]
    fn parallel_engines_cannot_reserve_the_same_nonce() {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..100)
                        .map(|_| reserve("thread test", [31; 32], 100, u64::MAX).unwrap())
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        let mut values: Vec<_> = handles
            .into_iter()
            .flat_map(|h| h.join().unwrap())
            .collect();
        values.sort_unstable();
        values.dedup();
        assert_eq!(values.len(), 800);
    }

    #[test]
    fn keys_normalise_public_addresses_and_separate_stable_ids() {
        let address = "0x00000000000000000000000000000000000000Ab";
        assert_eq!(
            NonceKey::ethereum_address(address),
            NonceKey::ethereum_address(&address[2..].to_ascii_lowercase())
        );
        // Byte-identical to the previous crypto::nonce agent domain key.
        assert_eq!(
            NonceKey::ethereum_address(address).0,
            <[u8; 32]>::from(Sha256::digest(address.to_ascii_lowercase().as_bytes()))
        );
        assert_ne!(
            NonceKey::stable_id(&address.to_ascii_lowercase()),
            NonceKey::ethereum_address(address)
        );
    }

    #[test]
    fn floors_reject_invalid_time_and_overflow() {
        assert_eq!(
            floor(1_700_000_000_000.9, 1, (1 << 53) - 1),
            Ok(1_700_000_000_000)
        );
        assert_eq!(floor(1_700.0, 1000, u64::MAX), Ok(1_700_000));
        for bad in [f64::NAN, f64::INFINITY, -1.] {
            assert_eq!(floor(bad, 1, u64::MAX), Err(NonceError::InvalidTime));
        }
        assert_eq!(floor(10., 1, 9), Err(NonceError::InvalidTime));
    }
}
