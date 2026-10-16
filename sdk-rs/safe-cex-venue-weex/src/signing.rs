//! WEEX API signing over the source string domain.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use sha2::Sha256;

pub fn sign_weex_message(secret: &[u16], message: &[u16]) -> String {
    // Node encodes UTF16 strings as UTF8, replacing each unpaired surrogate.
    let key = String::from_utf16_lossy(secret);
    let text = String::from_utf16_lossy(message);
    let mut mac = Hmac::<Sha256>::new_from_slice(key.as_bytes())
        .expect("HMAC-SHA256 accepts every key length");
    mac.update(text.as_bytes());
    STANDARD.encode(mac.finalize().into_bytes())
}
