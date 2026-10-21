//! Monotonic nonces shared by signers within a native process or one Wasm module.
//! Separate processes / Wasm memories must coordinate at their deployment boundary.
use std::{
    collections::HashMap,
    sync::{Mutex, OnceLock},
};

type NonceMap = HashMap<(&'static str, [u8; 32]), u64>;
static NONCES: OnceLock<Mutex<NonceMap>> = OnceLock::new();

/// `identity` is a public-key fingerprint or a one-way digest, never a raw secret.
/// State deliberately survives a signer dropping or an engine restarting.
pub fn next(
    namespace: &'static str,
    identity: [u8; 32],
    floor: u64,
    limit: u64,
) -> Result<u64, String> {
    let mut nonces = NONCES
        .get_or_init(Mutex::default)
        .lock()
        .map_err(|_| "Signing nonce state unavailable")?;
    let previous = nonces.entry((namespace, identity)).or_default();
    let nonce = floor.max(previous.checked_add(1).ok_or("Signing nonce exhausted")?);
    if nonce > limit {
        return Err("Signing nonce exhausted".into());
    }
    *previous = nonce;
    Ok(nonce)
}

/// Hyperliquid and TxFlow share the agent signer nonce domain, including vaults.
pub fn ethereum_agent(secret: &str, now: f64) -> Result<u64, String> {
    const LIMIT: u64 = (1 << 53) - 1;
    if !now.is_finite() || !(0.0..=LIMIT as f64).contains(&now) {
        return Err("Invalid agent signing time".into());
    }
    let address = crate::ethereum::private_key_address(secret)?.to_ascii_lowercase();
    next(
        "hyperliquid-agent",
        crate::sha256(address.as_bytes()),
        now.floor() as u64,
        LIMIT,
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn instances_and_restarts_share_a_monotonic_sequence_despite_clock_rollback() {
        let id = crate::sha256(b"restart test signer");
        assert_eq!(next("test", id, 100, 1000).unwrap(), 100);
        assert_eq!(next("test", id, 100, 1000).unwrap(), 101);
        assert_eq!(next("test", id, 0, 1000).unwrap(), 102);
        assert_eq!(
            next("test", crate::sha256(b"other signer"), 100, 1000).unwrap(),
            100
        );
        assert!(next("test", id, 1001, 1000).is_err());
        assert_eq!(next("test", id, 1, 1000).unwrap(), 103);
    }
    #[test]
    fn parallel_engines_cannot_reserve_the_same_nonce() {
        let handles: Vec<_> = (0..8)
            .map(|_| {
                std::thread::spawn(|| {
                    (0..100)
                        .map(|_| next("thread test", [31; 32], 100, u64::MAX).unwrap())
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
    fn agent_key_hex_case_does_not_create_a_new_nonce_domain() {
        let key = "00000000000000000000000000000000000000000000000000000000000000ab";
        let a = ethereum_agent(key, 1700000000000.0).unwrap();
        let b = ethereum_agent(&format!("0x{}", key.to_uppercase()), 1700000000000.0).unwrap();
        assert!(b > a);
        assert!(ethereum_agent(key, f64::NAN).is_err());
    }
}
