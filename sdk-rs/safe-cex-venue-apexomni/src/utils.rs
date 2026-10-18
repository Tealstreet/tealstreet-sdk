//! Correspondence: apexomni.utils.ts. Host randomness is injected once per call.
use crate::order_records::Iso8601;
use safe_cex_num::{js_number_string, number_from_string};

// ts: getPrecision
pub fn get_precision(num: &Iso8601) -> usize {
    let val = match num {
        Iso8601::Number(value) => *value,
        Iso8601::String(value) => number_from_string(value),
    };
    if val.is_nan() {
        return 0;
    }
    let text = match num {
        Iso8601::Number(value) => js_number_string(*value),
        Iso8601::String(value) => value.clone(),
    };
    let str_list: Vec<_> = text.split('.').collect();
    if str_list.len() == 2 {
        str_list[1].encode_utf16().count()
    } else {
        0
    }
}

// ts: generateApeXRandomClientId
pub fn generate_apex_random_client_id(random: impl FnOnce() -> f64) -> String {
    let text = js_number_string(random());
    let suffix = text.get(2..).unwrap_or_default().trim_start_matches('0');
    format!("apexomni-tealstreet-{suffix}")
}

// ts: dropPrefix
pub fn drop_prefix(hash: &str) -> &str {
    hash.strip_prefix("0x").unwrap_or(hash)
}

// ts: hmacSha256Base64. TextEncoder replaces each unpaired UTF-16 surrogate.
pub fn hmac_sha256_base64(message: &[u16], key: &[u16]) -> String {
    safe_cex_crypto::hmac_sha256(
        String::from_utf16_lossy(key).as_bytes(),
        String::from_utf16_lossy(message).as_bytes(),
        safe_cex_crypto::Encoding::Base64,
    )
}

#[cfg(test)]
mod accepted_scalar_helper_tests {
    use super::*;
    use crate::accepted_scalar_helper_support::*;
    #[test]
    fn drop_prefix_native_output() {
        check("utils::drop_prefix", 8, |value| {
            let input = value["text"].as_str().unwrap();
            let actual = drop_prefix(input);
            let offset = usize::try_from(value["offset"].as_u64().unwrap()).unwrap();
            assert_eq!(
                actual.as_ptr(),
                input.as_bytes()[offset..].as_ptr(),
                "literal borrowed slice identity"
            );
            serde_json::json!(actual)
        });
    }
}
