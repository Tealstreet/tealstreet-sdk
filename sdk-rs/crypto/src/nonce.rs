//! Kraken's legacy secret-digest nonce domain, served by the engine's single
//! store. New families use `Cx::reserve_nonce` with a `NonceKey`.

/// `identity` is a one-way digest, never a raw secret.
pub fn next(
    namespace: &'static str,
    identity: [u8; 32],
    floor: u64,
    limit: u64,
) -> Result<u64, String> {
    safe_cex_engine::nonce::reserve_legacy_digest(namespace, identity, floor, limit)
}
