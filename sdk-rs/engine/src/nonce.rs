//! Engine-owned monotonic signing nonces, shared by every engine in one native
//! process or Wasm module. Separate processes / Wasm memories must coordinate
//! at their deployment boundary. See docs/plans/RUST_CREDENTIAL_SELECTION.md (C3).
use sha2::{Digest, Sha256};
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

#[derive(Default)]
struct NonceState {
    reserved: u64,
    released: Option<u64>,
}
type NonceMap = HashMap<(&'static str, [u8; 32]), NonceState>;
static NONCES: OnceLock<Mutex<NonceMap>> = OnceLock::new();

/// A nonce domain's signer identity. Built only from non-secret material, so
/// no key digest or slot name can become a nonce domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonceKey([u8; 32]);

impl NonceKey {
    /// Trusted, caller-assigned `Identity.stable_id`; tagged apart from public ids.
    pub(crate) fn stable_id(id: &str) -> Self {
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

/// One family's nonce policy, declared once here rather than in venue code.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NonceDomain {
    name: &'static str,
    floor: NonceFloor,
    limit: u64,
}

/// Hyperliquid and TxFlow agents (including vaults) share one sequence per
/// agent address: raw host milliseconds within the safe-integer range.
pub const HYPERLIQUID_AGENT: NonceDomain = NonceDomain {
    name: "hyperliquid-agent",
    floor: NonceFloor::RawMs { scale: 1 },
    limit: (1 << 53) - 1,
};

/// Kraken Spot and Futures share a counter for the same trusted stable subject.
pub const KRAKEN: NonceDomain = NonceDomain {
    name: "kraken",
    floor: NonceFloor::RawMs { scale: 1000 },
    limit: u64::MAX,
};

#[path = "nonce_claim.rs"]
mod claim;
pub(crate) use claim::{ClaimAuthority, NonceReservation};
pub use claim::{NonceClaim, NonceSelectionError, NonceSubject};

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
    InvalidClaim,
    InvalidTime,
    Exhausted,
    Unavailable,
}

impl NonceError {
    pub fn message(&self) -> &'static str {
        match self {
            Self::InvalidClaim => "Invalid signing nonce claim",
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
    if !nonces.contains_key(&(domain, key)) {
        nonces.try_reserve(1).map_err(|_| NonceError::Unavailable)?;
    }
    let state = nonces.entry((domain, key)).or_default();
    let nonce = floor.max(state.reserved.checked_add(1).ok_or(NonceError::Exhausted)?);
    if nonce > limit {
        return Err(NonceError::Exhausted);
    }
    state.reserved = nonce;
    Ok(nonce)
}

/// Integer arithmetic after flooring, so large scaled floors stay exact.
pub(crate) fn floor(ms: f64, scale: u64, limit: u64) -> Result<u64, NonceError> {
    const TWO_POW_64: f64 = 18_446_744_073_709_551_616.0;
    if !ms.is_finite() || ms < 0. || ms.floor() >= TWO_POW_64 || scale == 0 {
        return Err(NonceError::InvalidTime);
    }
    (ms.floor() as u64)
        .checked_mul(scale)
        .filter(|scaled| *scaled <= limit)
        .ok_or(NonceError::InvalidTime)
}

impl<P> crate::Cx<'_, P> {
    /// Reserve a signing nonce before signing, in one shared monotonic domain.
    pub fn reserve_nonce(&self, domain: &NonceDomain, key: &NonceKey) -> Result<u64, NonceError> {
        let (ms, scale) = match domain.floor {
            NonceFloor::RawMs { scale } => (self.now, scale),
            NonceFloor::ProtocolMs { scale } => (self.protocol_now(), scale),
        };
        let floor = self::floor(ms, scale, domain.limit)?;
        reserve(domain.name, key.0, floor, domain.limit)
    }
}

/// A venue-issued nonce sequence (Lighter), one per engine, never above `LIMIT`.
/// It is seeded only from a validated venue reply requested under the current
/// ticket, reserved before signing, and invalidated on any refusal or doubt.
#[derive(Debug, Default)]
pub struct VenueCounter<const LIMIT: u64> {
    next: Option<u64>,
    epoch: u64,
}

/// Captured with a seed request; a reply after an invalidation cannot reseed.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct SeedTicket(u64);

impl<const LIMIT: u64> VenueCounter<LIMIT> {
    pub fn seeded(&self) -> bool {
        self.next.is_some()
    }

    pub fn ticket(&self) -> SeedTicket {
        SeedTicket(self.epoch)
    }

    /// Install a venue-validated next nonce; false if stale, seeded or above LIMIT.
    pub fn seed(&mut self, ticket: SeedTicket, value: u64) -> bool {
        let fresh = ticket.0 == self.epoch && self.next.is_none() && value <= LIMIT;
        if fresh {
            self.next = Some(value);
        }
        fresh
    }

    /// Reserve the next nonce; past LIMIT the counter is exhausted and unseeded.
    pub fn take(&mut self) -> Option<u64> {
        let nonce = self.next.filter(|nonce| *nonce <= LIMIT)?;
        self.next = nonce.checked_add(1);
        Some(nonce)
    }

    /// Forget the sequence; the next send must reseed from the venue.
    pub fn invalidate(&mut self) {
        self.next = None;
        self.epoch = self.epoch.wrapping_add(1);
    }
}

/// Kraken's secret-digest domain only, until its signer migration moves it to
/// `NonceKey::stable_id`. No other domain can be reached through this entry.
#[doc(hidden)]
pub fn reserve_kraken_legacy(identity: [u8; 32], floor: u64) -> Result<u64, String> {
    reserve("kraken", identity, floor, u64::MAX).map_err(|error| error.message().into())
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
    fn venue_counter_reserves_in_order_until_its_limit() {
        let mut counter = VenueCounter::<8>::default();
        assert_eq!(counter.take(), None);
        assert!(!counter.seed(counter.ticket(), 9), "above the limit");
        assert!(counter.seed(counter.ticket(), 7));
        assert!(!counter.seed(counter.ticket(), 7), "already seeded");
        assert_eq!((counter.take(), counter.take()), (Some(7), Some(8)));
        assert_eq!(counter.take(), None, "past the limit");
        let mut last = VenueCounter::<{ u64::MAX }>::default();
        assert!(last.seed(last.ticket(), u64::MAX));
        assert_eq!((last.take(), last.seeded()), (Some(u64::MAX), false));
    }

    #[test]
    fn venue_counter_drops_seeds_requested_before_an_invalidation() {
        let mut counter = VenueCounter::<100>::default();
        let stale = counter.ticket();
        counter.invalidate();
        assert!(!counter.seed(stale, 7));
        assert!(counter.seed(counter.ticket(), 7));
        counter.invalidate();
        assert_eq!(counter.take(), None);
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
        assert_eq!(floor(1., 0, u64::MAX), Err(NonceError::InvalidTime));
        // Exact above 2^53 and no saturation at the u64 boundary.
        assert_eq!(
            floor(1_700_000_000_000.0, 1_000_000, u64::MAX),
            Ok(1_700_000_000_000_000_000)
        );
        assert_eq!(
            floor(18_446_744_073_709_551_616.0, 1, u64::MAX),
            Err(NonceError::InvalidTime)
        );
        assert_eq!(
            floor(u32::MAX as f64, 1 << 33, u64::MAX),
            Err(NonceError::InvalidTime)
        );
    }
}
