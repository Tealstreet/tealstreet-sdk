//! Signed Tealstreet builder-fee exemptions for approved Lighter accounts.
//!
//! By default every attributed Lighter order carries Tealstreet's builder fee.
//! An exemption removes it for exactly one account, until a fixed expiry, and
//! only when it carries a valid Ed25519 signature from a key listed in [`KEYS`].
//! The signing key is generated and held offline by Tealstreet; only public
//! keys live here. Every malformed, unknown-key, wrong-account, wrong-network,
//! expired or over-long exemption is ignored, so the fee stays (fail closed).
//!
//! Token: `lbx1.<account index>.<expires unix s>.<key id>.<128 lowercase hex signature>`.
//! The signature covers [`payload`], which binds the purpose, network, account,
//! expiry and key id, so a token cannot be replayed on another account or network.

use safe_cex_crypto::ed25519;

/// Domain separation: these bytes start every signed payload.
pub const DOMAIN: &str = "tealstreet/lighter-builder-exemption/v1";

/// Accepted signing keys as (key id, Ed25519 public key). Empty until Tealstreet
/// adds its offline key, so until then no exemption is honoured. Changing this
/// list requires owner review; `keys_fingerprint` is pinned by a test.
pub const KEYS: &[(u32, [u8; 32])] = &[];

/// The longest validity accepted, even for a correctly signed token.
pub const MAX_LIFETIME_S: u64 = 400 * 86_400;

const MAX_TOKEN_LEN: usize = 256;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Exemption {
    pub account_index: u64,
    pub expires_s: u64,
    pub key_id: u32,
}

impl Exemption {
    /// True only for this exact account, before expiry, within the lifetime cap.
    pub fn covers(&self, account_index: Option<u64>, now_ms: f64) -> bool {
        let now_s = if now_ms.is_finite() && now_ms > 0.0 {
            (now_ms / 1000.0) as u64
        } else {
            return false;
        };
        account_index == Some(self.account_index)
            && now_s < self.expires_s
            && self.expires_s - now_s <= MAX_LIFETIME_S
    }
}

/// The exact bytes Tealstreet signs.
pub fn payload(network: &str, account_index: u64, expires_s: u64, key_id: u32) -> String {
    format!(
        "{DOMAIN}\nnetwork={network}\naccount={account_index}\nexpires={expires_s}\nkey={key_id}"
    )
}

/// Format a token from its fields and a 64-byte signature.
pub fn token(account_index: u64, expires_s: u64, key_id: u32, signature: &[u8; 64]) -> String {
    let hex: String = signature.iter().map(|b| format!("{b:02x}")).collect();
    format!("lbx1.{account_index}.{expires_s}.{key_id}.{hex}")
}

/// Canonical decimal: digits only, no sign, no leading zero (except "0"), bounded.
fn decimal(text: &str, max_digits: usize) -> Option<u64> {
    let canonical = !text.is_empty()
        && text.len() <= max_digits
        && text.bytes().all(|b| b.is_ascii_digit())
        && (text == "0" || !text.starts_with('0'));
    if canonical { text.parse().ok() } else { None }
}

fn signature(text: &str) -> Option<[u8; 64]> {
    if text.len() != 128 || !text.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f')) {
        return None;
    }
    let mut out = [0u8; 64];
    for (i, pair) in text.as_bytes().chunks(2).enumerate() {
        out[i] = u8::from_str_radix(std::str::from_utf8(pair).ok()?, 16).ok()?;
    }
    Some(out)
}

/// Parse and verify a token for `network` against `keys`. Expiry and account
/// are checked at use time with [`Exemption::covers`].
pub fn verify(
    token: &str,
    network: &str,
    keys: &[(u32, [u8; 32])],
) -> Result<Exemption, &'static str> {
    if token.len() > MAX_TOKEN_LEN || !token.is_ascii() {
        return Err("malformed exemption");
    }
    let parts: Vec<&str> = token.split('.').collect();
    let [version, account, expires, key, sig] = parts[..] else {
        return Err("malformed exemption");
    };
    if version != "lbx1" {
        return Err("unknown exemption version");
    }
    let (Some(account_index), Some(expires_s), Some(key_id), Some(sig)) = (
        decimal(account, 20),
        decimal(expires, 12),
        decimal(key, 9),
        signature(sig),
    ) else {
        return Err("malformed exemption");
    };
    let key_id = u32::try_from(key_id).map_err(|_| "malformed exemption")?;
    let Some((_, public_key)) = keys.iter().find(|(id, _)| *id == key_id) else {
        return Err("unknown exemption key");
    };
    let signed = payload(network, account_index, expires_s, key_id);
    match ed25519::verify(signed.as_bytes(), public_key, &sig) {
        Ok(true) => Ok(Exemption {
            account_index,
            expires_s,
            key_id,
        }),
        _ => Err("invalid exemption signature"),
    }
}

/// A short, stable fingerprint of [`KEYS`] (FNV-1a over ids and keys), pinned by
/// a test so a key change is always a deliberate, reviewed edit.
pub fn keys_fingerprint(keys: &[(u32, [u8; 32])]) -> u64 {
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for (id, key) in keys {
        for byte in id.to_le_bytes().iter().chain(key.iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0100_0000_01b3);
        }
    }
    hash
}

#[cfg(test)]
mod tests {
    use super::*;

    const SEED: [u8; 32] = [7; 32];
    const OTHER_SEED: [u8; 32] = [9; 32];
    const NOW_MS: f64 = 1_795_000_000_000.0;
    const EXPIRES: u64 = 1_795_000_000 + 90 * 86_400;

    fn keys() -> Vec<(u32, [u8; 32])> {
        vec![(1, ed25519::public_key(&SEED).unwrap())]
    }

    fn signed(seed: &[u8; 32], network: &str, account: u64, expires: u64, key_id: u32) -> String {
        let sig =
            ed25519::sign(payload(network, account, expires, key_id).as_bytes(), seed).unwrap();
        token(account, expires, key_id, &sig)
    }

    #[test]
    fn a_valid_exemption_covers_only_its_account_until_expiry() {
        let ex = verify(
            &signed(&SEED, "mainnet", 767_227, EXPIRES, 1),
            "mainnet",
            &keys(),
        )
        .unwrap();
        assert!(ex.covers(Some(767_227), NOW_MS));
        assert!(!ex.covers(Some(767_228), NOW_MS));
        assert!(!ex.covers(None, NOW_MS));
        assert!(!ex.covers(Some(767_227), EXPIRES as f64 * 1000.0));
        assert!(!ex.covers(Some(767_227), f64::NAN));
        assert!(!ex.covers(Some(767_227), 0.0));
    }

    #[test]
    fn over_long_lifetimes_are_refused_even_when_signed() {
        let far = 1_795_000_000 + MAX_LIFETIME_S + 1;
        let ex = verify(&signed(&SEED, "mainnet", 5, far, 1), "mainnet", &keys()).unwrap();
        assert!(!ex.covers(Some(5), NOW_MS));
    }

    #[test]
    fn tampering_wrong_key_network_or_unknown_key_is_refused() {
        let good = signed(&SEED, "mainnet", 767_227, EXPIRES, 1);
        // Any edited field breaks the signature.
        let other_account = good.replacen("lbx1.767227.", "lbx1.767228.", 1);
        assert_eq!(
            verify(&other_account, "mainnet", &keys()),
            Err("invalid exemption signature")
        );
        let later = good.replacen(&format!(".{EXPIRES}."), &format!(".{}.", EXPIRES + 1), 1);
        assert_eq!(
            verify(&later, "mainnet", &keys()),
            Err("invalid exemption signature")
        );
        // Signed for another network.
        assert_eq!(
            verify(&good, "robinhood", &keys()),
            Err("invalid exemption signature")
        );
        // Signed by a key that is not listed, or naming an unlisted key id.
        let forged = signed(&OTHER_SEED, "mainnet", 767_227, EXPIRES, 1);
        assert_eq!(
            verify(&forged, "mainnet", &keys()),
            Err("invalid exemption signature")
        );
        let unknown = signed(&SEED, "mainnet", 767_227, EXPIRES, 2);
        assert_eq!(
            verify(&unknown, "mainnet", &keys()),
            Err("unknown exemption key")
        );
        // With no keys listed (the shipped default), nothing verifies.
        assert_eq!(verify(&good, "mainnet", KEYS), Err("unknown exemption key"));
    }

    #[test]
    fn non_canonical_tokens_are_refused() {
        let good = signed(&SEED, "mainnet", 767_227, EXPIRES, 1);
        let sig = good.rsplit('.').next().unwrap().to_owned();
        let cases = [
            String::new(),
            "lbx1".into(),
            good.replacen("lbx1.", "lbx2.", 1),
            good.replacen("lbx1.767227.", "lbx1.0767227.", 1),
            good.replacen("lbx1.767227.", "lbx1.+767227.", 1),
            good.replacen("lbx1.767227.", "lbx1.767227 .", 1),
            format!("{good}."),
            format!("{good}.extra"),
            good.replace(&sig, &sig.to_uppercase()),
            good.replace(&sig, &sig[..126]),
            format!("lbx1.1.1.1.{}", "0".repeat(300)),
            good.replacen("lbx1.", "lbx1.é", 1),
        ];
        for token in cases {
            assert!(verify(&token, "mainnet", &keys()).is_err(), "{token}");
        }
    }

    #[test]
    fn shipped_keys_are_pinned() {
        // Update this only together with a reviewed change to KEYS.
        assert_eq!(keys_fingerprint(KEYS), 0xcbf2_9ce4_8422_2325);
    }
}
