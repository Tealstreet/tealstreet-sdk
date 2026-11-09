//! Single-use handovers on an authenticated hosted connection.
use base64::{Engine as _, engine::general_purpose::STANDARD};
use hpke::{
    Deserializable, OpModeR, aead::ChaCha20Poly1305, kdf::HkdfSha256, kem::X25519HkdfSha256,
};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use zeroize::Zeroizing;

pub const INFO: &[u8] = b"tealstreet/delegated-handover/v1";
pub const HANDOVER_TTL_MS: u64 = 30_000;
const MAX_ATTEMPTS: usize = 64;
const MAX_CIPHERTEXT: usize = 16 * 1024;

/// This slice reads the shared opt-in declaration. No venue is enabled by
/// default; the account-build slice supplies Bybit's declaration.
pub fn enabled(venue: &str) -> bool {
    static RULES: std::sync::OnceLock<serde_json::Value> = std::sync::OnceLock::new();
    let rules = RULES.get_or_init(|| {
        serde_json::from_str(include_str!(
            "../../../../packages/safe-cex/src/exchanges/rust/construction-rules.json"
        ))
        .expect("construction rules parse")
    });
    venue == "bybit" && rules["venues"][venue]["delegated"]["enabled"] == true
}

#[derive(Clone, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct Sealed {
    pub attempt_id: String,
    pub enc: String,
    pub ciphertext: String,
}

// Envelopes may be included in diagnostics; ciphertext and encapsulations need
// never appear there, even though no plaintext is present.
impl std::fmt::Debug for Sealed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Sealed { .. }")
    }
}

/// Each field owns an erasing allocation, including on a partial JSON parse.
/// Intentionally neither Debug, Clone nor Serialize.
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct Credentials {
    #[serde(deserialize_with = "credential")]
    pub key: Zeroizing<String>,
    #[serde(deserialize_with = "credential")]
    pub secret: Zeroizing<String>,
    #[serde(default, deserialize_with = "optional_credential")]
    pub passphrase: Option<Zeroizing<String>>,
}

fn credential<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Zeroizing<String>, D::Error> {
    struct Text;
    impl serde::de::Visitor<'_> for Text {
        type Value = Zeroizing<String>;
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("a string")
        }
        fn visit_str<E: serde::de::Error>(self, text: &str) -> Result<Self::Value, E> {
            Ok(Zeroizing::new(text.to_owned()))
        }
        fn visit_string<E: serde::de::Error>(self, text: String) -> Result<Self::Value, E> {
            Ok(Zeroizing::new(text))
        }
    }
    d.deserialize_string(Text)
}

fn optional_credential<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<Zeroizing<String>>, D::Error> {
    #[derive(Deserialize)]
    struct Text(#[serde(deserialize_with = "credential")] Zeroizing<String>);
    Ok(Option::<Text>::deserialize(d)?.map(|text| text.0))
}

/// Deliberately contains no parser or crypto diagnostic or input fragment.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused;

impl std::fmt::Display for Refused {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Could not verify your key; account not attached")
    }
}

/// Public response fields, flattened into the existing `msg` envelope.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct KeyOffer {
    pub attempt_id: String,
    pub public_key: String,
    pub boot_id: &'static str,
    pub expires_at_ms: u64,
}

struct Attempt {
    id: String,
    private: Zeroizing<[u8; 32]>,
    expires: u64,
}

/// Owned by one Session, never stored in a process/user-wide registry.
#[derive(Default)]
pub struct Handovers {
    attempts: HashMap<String, Attempt>,
}

impl Handovers {
    pub fn expire(&mut self, now: u64) {
        self.attempts.retain(|_, attempt| attempt.expires > now);
    }

    pub fn issue(&mut self, account: &str, now: u64) -> Result<KeyOffer, Refused> {
        self.expire(now);
        if account.is_empty()
            || (!self.attempts.contains_key(account) && self.attempts.len() >= MAX_ATTEMPTS)
        {
            return Err(Refused);
        }
        let mut private = Zeroizing::new([0u8; 32]);
        getrandom::getrandom(private.as_mut()).map_err(|_| Refused)?;
        let mut id = [0u8; 16];
        getrandom::getrandom(&mut id).map_err(|_| Refused)?;
        let id = id
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect::<String>();
        let public = x25519_dalek::PublicKey::from(&x25519_dalek::StaticSecret::from(*private));
        let expires = now.saturating_add(HANDOVER_TTL_MS);
        let offer = KeyOffer {
            attempt_id: id.clone(),
            public_key: STANDARD.encode(public.as_bytes()),
            boot_id: crate::state::boot_id(),
            expires_at_ms: expires,
        };
        self.attempts.insert(
            account.to_owned(),
            Attempt {
                id,
                private,
                expires,
            },
        );
        Ok(offer)
    }

    pub fn forget(&mut self, account: &str) {
        self.attempts.remove(account);
    }

    /// Consumes the matching attempt before decoding or opening any input.
    pub fn take(&mut self, account: &str, sealed: &Sealed, now: u64) -> Result<Opening, Refused> {
        let attempt = self.attempts.get(account).ok_or(Refused)?;
        if attempt.id != sealed.attempt_id {
            return Err(Refused);
        }
        let attempt = self.attempts.remove(account).ok_or(Refused)?;
        if attempt.expires <= now {
            return Err(Refused);
        }
        Ok(Opening {
            attempt,
            boot_id: crate::state::boot_id(),
        })
    }
}

/// An owned, consumed handover, suitable for carrying into a join-proof task.
pub struct Opening {
    attempt: Attempt,
    boot_id: &'static str,
}

impl Opening {
    /// Opens once, with both plaintext storage and parsed fields erased on drop.
    /// The probe slice calls this before constructing its candidate account.
    pub fn open(
        self,
        exchange: &str,
        key: &str,
        sealed: &Sealed,
        now: u64,
    ) -> Result<Credentials, Refused> {
        if self.attempt.expires <= now || sealed.attempt_id != self.attempt.id {
            return Err(Refused);
        }
        let aad = format!("{}|{}|{exchange}|{key}", self.attempt.id, self.boot_id);
        let plaintext = open(&self.attempt.private[..], INFO, aad.as_bytes(), sealed)?;
        let credentials: Credentials = serde_json::from_slice(&plaintext).map_err(|_| Refused)?;
        if credentials.key.as_str() != key || key.is_empty() || credentials.secret.is_empty() {
            return Err(Refused);
        }
        Ok(credentials)
    }
}

fn open(
    private: &[u8],
    info: &[u8],
    aad: &[u8],
    sealed: &Sealed,
) -> Result<Zeroizing<Vec<u8>>, Refused> {
    use hpke::aead::AeadTag;
    type Kem = X25519HkdfSha256;
    if sealed.enc.len() > 44 || sealed.ciphertext.len() > MAX_CIPHERTEXT {
        return Err(Refused);
    }
    let enc = STANDARD.decode(&sealed.enc).map_err(|_| Refused)?;
    let enc = <Kem as hpke::Kem>::EncappedKey::from_bytes(&enc).map_err(|_| Refused)?;
    let private = <Kem as hpke::Kem>::PrivateKey::from_bytes(private).map_err(|_| Refused)?;
    let mut receiver = hpke::setup_receiver::<ChaCha20Poly1305, HkdfSha256, Kem>(
        &OpModeR::Base,
        &private,
        &enc,
        info,
    )
    .map_err(|_| Refused)?;
    let mut plaintext = Zeroizing::new(STANDARD.decode(&sealed.ciphertext).map_err(|_| Refused)?);
    let tag_start = plaintext.len().checked_sub(16).ok_or(Refused)?;
    let tag =
        AeadTag::<ChaCha20Poly1305>::from_bytes(&plaintext[tag_start..]).map_err(|_| Refused)?;
    plaintext.truncate(tag_start);
    receiver
        .open_in_place_detached(plaintext.as_mut_slice(), aad, &tag)
        .map_err(|_| Refused)?;
    Ok(plaintext)
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use hpke::{Kem as _, Serializable};

    /// Fixed synthetic sender randomness for cross-implementation test data only.
    struct FixedRandom(u8);
    impl hpke::rand_core::RngCore for FixedRandom {
        fn next_u32(&mut self) -> u32 {
            panic!("fill_bytes only")
        }
        fn next_u64(&mut self) -> u64 {
            panic!("fill_bytes only")
        }
        fn fill_bytes(&mut self, dest: &mut [u8]) {
            dest.fill(self.0);
        }
    }
    impl hpke::rand_core::CryptoRng for FixedRandom {}

    pub(crate) fn seal(offer: &KeyOffer, exchange: &str, key: &str, plaintext: &[u8]) -> Sealed {
        let public = <X25519HkdfSha256 as hpke::Kem>::PublicKey::from_bytes(
            &STANDARD.decode(&offer.public_key).unwrap(),
        )
        .unwrap();
        let (enc, mut sender) =
            hpke::setup_sender::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256, _>(
                &hpke::OpModeS::Base,
                &public,
                INFO,
                &mut FixedRandom(0x42),
            )
            .unwrap();
        let aad = format!("{}|{}|{exchange}|{key}", offer.attempt_id, offer.boot_id);
        Sealed {
            attempt_id: offer.attempt_id.clone(),
            enc: STANDARD.encode(enc.to_bytes()),
            ciphertext: STANDARD.encode(sender.seal(plaintext, aad.as_bytes()).unwrap()),
        }
    }

    fn hex(text: &str) -> Vec<u8> {
        text.as_bytes()
            .chunks_exact(2)
            .map(|pair| u8::from_str_radix(std::str::from_utf8(pair).unwrap(), 16).unwrap())
            .collect()
    }

    #[test]
    fn fixed_cross_implementation_handover_matches_independent_fixture() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/delegated-handover-v1.json")).unwrap();
        let sealed: Sealed = serde_json::from_value(v["sealed"].clone()).unwrap();
        let private: [u8; 32] = STANDARD
            .decode(v["recipientPrivateKey"].as_str().unwrap())
            .unwrap()
            .try_into()
            .unwrap();
        let offer = KeyOffer {
            attempt_id: v["attemptId"].as_str().unwrap().into(),
            public_key: v["publicKey"].as_str().unwrap().into(),
            boot_id: "00000000-0000-4000-8000-000000000001",
            expires_at_ms: v["expiresAtMs"].as_u64().unwrap(),
        };
        let key = v["key"].as_str().unwrap();
        let generated = seal(
            &offer,
            "bybit",
            key,
            v["plaintext"].as_str().unwrap().as_bytes(),
        );
        assert!(generated.enc == sealed.enc && generated.ciphertext == sealed.ciphertext);
        let opening = Opening {
            attempt: Attempt {
                id: offer.attempt_id,
                private: Zeroizing::new(private),
                expires: offer.expires_at_ms,
            },
            boot_id: offer.boot_id,
        };
        let candidate = opening.open("bybit", key, &sealed, 100).unwrap();
        assert!(candidate.key.as_str() == key);
        assert!(candidate.secret.as_str() == "synthetic-api-secret");
        assert!(
            candidate.passphrase.as_deref().map(String::as_str) == Some("synthetic-passphrase")
        );
    }

    #[test]
    fn handover_is_disabled_for_every_other_venue() {
        for venue in [
            "bybitspot",
            "bybitinverse",
            "bybitdemo",
            "okx",
            "kraken",
            "",
            "BYBIT",
        ] {
            assert!(!enabled(venue), "{venue}");
        }
    }

    #[test]
    fn rfc9180_base_mode_known_answers_open_in_sequence() {
        let v: serde_json::Value =
            serde_json::from_str(include_str!("../tests/data/rfc9180-x25519-chacha20.json"))
                .unwrap();
        let bytes = |field: &str| hex(v[field].as_str().unwrap());
        let (private, public) = X25519HkdfSha256::derive_keypair(&bytes("ikmR"));
        assert!(public.to_bytes()[..] == bytes("pkRm"));
        let enc = <X25519HkdfSha256 as hpke::Kem>::EncappedKey::from_bytes(&bytes("enc")).unwrap();
        let mut receiver = hpke::setup_receiver::<ChaCha20Poly1305, HkdfSha256, X25519HkdfSha256>(
            &OpModeR::Base,
            &private,
            &enc,
            &bytes("info"),
        )
        .unwrap();
        for answer in v["encryptions"].as_array().unwrap() {
            let mut ciphertext = Zeroizing::new(hex(answer["ct"].as_str().unwrap()));
            let tag_start = ciphertext.len() - 16;
            let tag = hpke::aead::AeadTag::<ChaCha20Poly1305>::from_bytes(&ciphertext[tag_start..])
                .unwrap();
            ciphertext.truncate(tag_start);
            receiver
                .open_in_place_detached(
                    &mut ciphertext,
                    &hex(answer["aad"].as_str().unwrap()),
                    &tag,
                )
                .unwrap();
            assert!(ciphertext.as_slice() == hex(answer["pt"].as_str().unwrap()));
        }
        // Also exercise the production opener, which creates a fresh seq=0 context.
        let first = &v["encryptions"][0];
        let sealed = Sealed {
            attempt_id: "rfc-test".into(),
            enc: STANDARD.encode(bytes("enc")),
            ciphertext: STANDARD.encode(hex(first["ct"].as_str().unwrap())),
        };
        assert!(
            open(
                &private.to_bytes(),
                &bytes("info"),
                &hex(first["aad"].as_str().unwrap()),
                &sealed
            )
            .unwrap()
            .as_slice()
                == hex(first["pt"].as_str().unwrap())
        );
    }

    #[test]
    fn attempts_are_single_use_account_connection_and_deadline_bound() {
        let mut owner = Handovers::default();
        let offer = owner.issue("account", 100).unwrap();
        let sealed = seal(
            &offer,
            "bybit",
            "synthetic-key",
            br#"{"key":"synthetic-key","secret":"synthetic-secret","passphrase":"test"}"#,
        );
        assert!(owner.take("other-account", &sealed, 101).is_err());
        assert!(Handovers::default().take("account", &sealed, 101).is_err());
        let opening = owner.take("account", &sealed, 101).unwrap();
        assert!(owner.take("account", &sealed, 102).is_err());
        let candidate = opening
            .open("bybit", "synthetic-key", &sealed, 102)
            .unwrap();
        assert!(candidate.secret.as_str() == "synthetic-secret");
        assert!(candidate.passphrase.as_deref().map(String::as_str) == Some("test"));
        let offer = owner.issue("account", 200).unwrap();
        let sealed = seal(&offer, "bybit", "synthetic-key", b"{}");
        assert!(
            owner
                .take("account", &sealed, 200 + HANDOVER_TTL_MS)
                .is_err()
        );
    }

    #[test]
    fn replacement_and_every_authenticated_context_field_are_checked() {
        let mut owner = Handovers::default();
        let first = owner.issue("account", 100).unwrap();
        let old = seal(&first, "bybit", "synthetic-key", b"{}");
        owner.issue("account", 101).unwrap();
        assert!(owner.take("account", &old, 102).is_err());
        for change in [
            "boot",
            "venue",
            "key",
            "payload-key",
            "info",
            "tag",
            "enc",
            "attempt",
            "expired",
        ] {
            let offer = owner.issue("account", 100).unwrap();
            let plaintext: &[u8] = if change == "payload-key" {
                br#"{"key":"different","secret":"synthetic"}"#
            } else {
                br#"{"key":"synthetic-key","secret":"synthetic"}"#
            };
            let mut sealed = seal(&offer, "bybit", "synthetic-key", plaintext);
            let mut opening = owner.take("account", &sealed, 101).unwrap();
            let (mut venue, mut key, mut now) = ("bybit", "synthetic-key", 102);
            match change {
                "boot" => opening.boot_id = "another-boot",
                "venue" => venue = "okx",
                "key" => key = "another-key",
                "tag" => {
                    let mut data = STANDARD.decode(&sealed.ciphertext).unwrap();
                    data[0] ^= 1;
                    sealed.ciphertext = STANDARD.encode(data);
                }
                "enc" => sealed.enc = STANDARD.encode([0u8; 32]),
                "attempt" => sealed.attempt_id.push('x'),
                "expired" => now = 100 + HANDOVER_TTL_MS,
                "info" => {
                    let aad = format!("{}|{}|bybit|synthetic-key", offer.attempt_id, offer.boot_id);
                    assert!(
                        open(
                            &opening.attempt.private[..],
                            b"different-info",
                            aad.as_bytes(),
                            &sealed
                        )
                        .is_err()
                    );
                    continue;
                }
                _ => {}
            }
            assert!(opening.open(venue, key, &sealed, now).is_err(), "{change}");
        }
    }

    #[test]
    fn malformed_plaintext_and_encodings_are_generic_refusals() {
        for plaintext in [
            b"not-json".as_slice(),
            br#"{"key":"synthetic-key","secret":""}"#,
            br#"{"key":"synthetic-key","secret":3}"#,
            br#"{"key":"synthetic-key","secret":"synthetic","extra":true}"#,
        ] {
            let mut owner = Handovers::default();
            let offer = owner.issue("account", 100).unwrap();
            let sealed = seal(&offer, "bybit", "synthetic-key", plaintext);
            assert!(
                owner
                    .take("account", &sealed, 101)
                    .unwrap()
                    .open("bybit", "synthetic-key", &sealed, 102)
                    .is_err()
            );
        }
        for bad in ["invalid-base64", "", "AA=="] {
            let mut owner = Handovers::default();
            let offer = owner.issue("account", 100).unwrap();
            let mut sealed = seal(&offer, "bybit", "synthetic-key", b"{}");
            sealed.ciphertext = bad.into();
            assert!(
                owner
                    .take("account", &sealed, 101)
                    .unwrap()
                    .open("bybit", "synthetic-key", &sealed, 102)
                    .is_err()
            );
            assert!(owner.take("account", &sealed, 103).is_err());
        }
    }

    #[test]
    fn offers_are_bounded_replaced_and_removed_on_dispose() {
        let mut owner = Handovers::default();
        for i in 0..MAX_ATTEMPTS {
            owner.issue(&i.to_string(), 100).unwrap();
        }
        assert!(owner.issue("overflow", 100).is_err());
        owner.issue("0", 100).unwrap();
        owner.forget("0");
        owner.issue("new", 100).unwrap();
        owner
            .issue("expired-reclaimed", 100 + HANDOVER_TTL_MS)
            .unwrap();
        assert_eq!(owner.attempts.len(), 1);
    }
}
