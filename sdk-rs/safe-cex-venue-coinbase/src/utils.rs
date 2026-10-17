//! Pure helpers from coinbase.utils.ts.
use safe_cex_types::{OrderStatus, OrderType};
use serde_json::{Map, Value};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CoinbaseSignatureAlgorithm {
    Es256,
    EdDsa,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct NormalizedCoinbaseApiCredentials {
    pub algorithm: CoinbaseSignatureAlgorithm,
    pub ed25519_seed: Option<Vec<u8>>,
    pub key_name: String,
    pub key_secret: String,
}

// ts: normalizeCoinbaseUsdEquivalentProductId
pub fn normalize_coinbase_usd_equivalent_product_id(product_id: &str) -> String {
    if product_id.ends_with("-USDC") && !matches!(product_id, "USDT-USDC" | "EURC-USDC") {
        return format!("{}-USD", &product_id[..product_id.len() - 5]);
    }
    product_id.to_owned()
}

// ts: getCoinbaseUsdEquivalentAliasProductId
pub fn get_coinbase_usd_equivalent_alias_product_id(product_id: &str) -> Option<String> {
    let normalized = normalize_coinbase_usd_equivalent_product_id(product_id);
    if normalized != product_id {
        return Some(normalized);
    }
    product_id
        .strip_suffix("-USD")
        .map(|base| format!("{base}-USDC"))
}

// ts: resolveCoinbaseUsdEquivalentMarket
// ts-bug: resolveCoinbaseUsdEquivalentMarket accepts inherited Object.prototype keys.
// CORRECTED: a JSON map has only own market values; see the manifest's alternative.
pub fn resolve_coinbase_usd_equivalent_market<'a>(
    product_id: &str,
    markets_by_id: &'a Map<String, Value>,
) -> Option<&'a Value> {
    let exact = markets_by_id.get(product_id);
    let alias_id = get_coinbase_usd_equivalent_alias_product_id(product_id);
    let alias = alias_id.as_ref().and_then(|id| markets_by_id.get(id));
    if product_id.ends_with("-USD") && alias.is_some_and(truthy) {
        return alias;
    }
    exact.filter(|v| !v.is_null()).or(alias)
}

pub(crate) fn truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => value.as_f64().is_none_or(|v| v != 0.0 && !v.is_nan()),
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

// ts: formatOrderState
pub fn format_order_state(
    raw_state: &str,
    mut console_error: impl FnMut(&str, &str),
) -> OrderStatus {
    match raw_state {
        "PENDING" | "OPEN" => OrderStatus::Open,
        "FILLED" => OrderStatus::Closed,
        "CANCELLED" | "EXPIRED" | "FAILED" => OrderStatus::Canceled,
        _ => {
            console_error("Unknown order state", raw_state);
            OrderStatus::Open
        }
    }
}

// ts: formatOrderType
pub fn format_order_type(raw_type: &str, mut console_error: impl FnMut(&str, &str)) -> OrderType {
    match raw_type {
        "Market" => OrderType::Market,
        "Limit" => OrderType::Limit,
        "Stop Limit" => OrderType::StopLoss,
        _ => {
            console_error("Unknown order type", raw_type);
            OrderType::Limit
        }
    }
}

// ts: normalizeCfmContractSize
pub fn normalize_cfm_contract_size(value: &Value) -> Option<f64> {
    let size = match value {
        Value::Number(value) => value.as_f64().unwrap_or(f64::NAN),
        Value::String(value) => safe_cex_num::number_from_string(value),
        _ => f64::NAN,
    };
    (size.is_finite() && size > 0.0).then_some(size)
}

// ts: getCoinbaseJsonSecret
pub fn get_coinbase_json_secret(value: Option<&Value>) -> Option<&str> {
    let value = value?;
    [
        "privateKey",
        "private_key",
        "keySecret",
        "key_secret",
        "secret",
    ]
    .into_iter()
    .find_map(|key| value.get(key).and_then(Value::as_str))
}

// ts: parseCoinbaseCredentialJson
pub fn parse_coinbase_credential_json(value: &str) -> Option<Value> {
    let trimmed = safe_cex_num::js_trim(value);
    if !trimmed.starts_with('{') {
        return None;
    }
    let parsed: Value = serde_json::from_str(trimmed).ok()?;
    parsed.is_object().then_some(parsed)
}

fn coinbase_pem_contains_private_key(value: &str) -> bool {
    const BEGIN: [&str; 2] = [
        "-----BEGIN PRIVATE KEY-----",
        "-----BEGIN EC PRIVATE KEY-----",
    ];
    const END: [&str; 2] = ["-----END PRIVATE KEY-----", "-----END EC PRIVATE KEY-----"];
    BEGIN.iter().any(|begin| {
        value.match_indices(begin).any(|(start, _)| {
            let content = &value[start + begin.len()..];
            END.iter().any(|end| {
                content
                    .match_indices(end)
                    .any(|(end_start, _)| end_start > 0)
            })
        })
    })
}

fn is_coinbase_key_name(value: &str) -> bool {
    let Some(rest) = value.strip_prefix("organizations/") else {
        return false;
    };
    let Some((organization, rest)) = rest.split_once("/apiKeys/") else {
        return false;
    };
    !organization.is_empty()
        && !organization.contains('/')
        && !rest.is_empty()
        && !rest.contains('/')
}

// ts: normalizeCoinbaseApiCredentials
pub fn normalize_coinbase_api_credentials(
    key: &str,
    secret: &str,
) -> Result<NormalizedCoinbaseApiCredentials, &'static str> {
    const INVALID_NAME: &str = "Coinbase API key must be the Advanced Trade CDP key name, like organizations/{org_id}/apiKeys/{key_id}.";
    const INVALID_SECRET: &str = "Coinbase API secret must be the Advanced Trade private key: either an ES256 privateKey PEM or an Ed25519 base64 key secret from Coinbase.";

    let key_json = parse_coinbase_credential_json(key);
    let secret_json = parse_coinbase_credential_json(secret);
    let key_name = key_json
        .as_ref()
        .and_then(|value| value.get("name"))
        .and_then(Value::as_str)
        .unwrap_or(key);
    let key_name = safe_cex_num::js_trim(key_name).to_owned();
    let selected_secret = get_coinbase_json_secret(secret_json.as_ref())
        .or_else(|| get_coinbase_json_secret(key_json.as_ref()))
        .unwrap_or(secret);
    let key_secret = safe_cex_num::js_trim(&selected_secret.replace("\\n", "\n")).to_owned();

    if !is_coinbase_key_name(&key_name) {
        return Err(INVALID_NAME);
    }
    if coinbase_pem_contains_private_key(&key_secret) {
        return Ok(NormalizedCoinbaseApiCredentials {
            algorithm: CoinbaseSignatureAlgorithm::Es256,
            ed25519_seed: None,
            key_name,
            key_secret,
        });
    }
    let ed25519_secret = base64_to_bytes(&key_secret);
    if let Some(bytes) = ed25519_secret.filter(|bytes| matches!(bytes.len(), 32 | 64)) {
        return Ok(NormalizedCoinbaseApiCredentials {
            algorithm: CoinbaseSignatureAlgorithm::EdDsa,
            ed25519_seed: Some(bytes[..32].to_vec()),
            key_name,
            key_secret,
        });
    }
    Err(INVALID_SECRET)
}

// ts: base64ToBytes
pub fn base64_to_bytes(value: &str) -> Option<Vec<u8>> {
    let value = if value
        .get(..8)
        .is_some_and(|v| v.eq_ignore_ascii_case("ed25519:"))
    {
        &value[8..]
    } else {
        value
    };
    let mut normalized = safe_cex_num::js_split_whitespace(value)
        .concat()
        .replace('-', "+")
        .replace('_', "/");
    if normalized.is_empty()
        || !normalized
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'+' | b'/' | b'='))
    {
        return None;
    }
    let padding = (4 - normalized.len() % 4) % 4;
    normalized.extend(std::iter::repeat_n('=', padding));
    safe_cex_transport::codec::decode_base64(&normalized).ok()
}

// ts: base64UrlEncode
pub fn base64_url_encode(data: &[u8]) -> String {
    use base64::Engine;
    base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(data)
}

// ts: base64UrlEncodeString
pub fn base64_url_encode_string(value: &str) -> String {
    base64_url_encode(value.as_bytes())
}

// ts: asn1Wrap
pub fn asn1_wrap(tag: f64, data: &[u8]) -> Vec<u8> {
    // Uint8Array converts the numeric tag modulo 256, truncating toward zero.
    let tag = tag.trunc().rem_euclid(256.0) as u8;
    let len = data.len();
    let mut result = if len < 128 {
        vec![tag, len as u8]
    } else if len < 256 {
        vec![tag, 0x81, len as u8]
    } else {
        // Retain the source's two-byte length, including truncation beyond 65535.
        vec![tag, 0x82, ((len >> 8) & 0xff) as u8, (len & 0xff) as u8]
    };
    result.extend_from_slice(data);
    result
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CoinbasePemDecodeError;

// ts: pemToPkcs8Der
pub fn pem_to_pkcs8_der(pem: &str) -> Result<Vec<u8>, CoinbasePemDecodeError> {
    pem_to_pkcs8_der_with(pem, |encoded| {
        safe_cex_transport::codec::decode_base64(encoded).map_err(|_| CoinbasePemDecodeError)
    })
}

/// The source body with its imported atob operation supplied separately. Native
/// assembly keeps the decoder's original failure instead of rebuilding the pure
/// facade's deliberately metadata-free unit error.
pub(crate) fn pem_to_pkcs8_der_with<E>(
    pem: &str,
    decode: impl FnOnce(&str) -> Result<Vec<u8>, E>,
) -> Result<Vec<u8>, E> {
    let is_pkcs8 = pem.contains("-----BEGIN PRIVATE KEY-----");
    // JS replace without /g removes only the first matching marker of each kind.
    let remove_first = |text: String, first: &str, second: &str| {
        let found = [first, second]
            .into_iter()
            .filter_map(|marker| text.find(marker).map(|offset| (offset, marker)))
            .min_by_key(|(offset, _)| *offset);
        match found {
            Some((_, marker)) => text.replacen(marker, "", 1),
            None => text,
        }
    };
    let b64 = remove_first(
        pem.to_owned(),
        "-----BEGIN EC PRIVATE KEY-----",
        "-----BEGIN PRIVATE KEY-----",
    );
    let b64 = remove_first(
        b64,
        "-----END EC PRIVATE KEY-----",
        "-----END PRIVATE KEY-----",
    );
    let b64 = safe_cex_num::js_split_whitespace(&b64).concat();
    let der = decode(&b64)?;
    if is_pkcs8 {
        return Ok(der);
    }
    let version = [0x02, 0x01, 0x00];
    let alg_id = [
        0x30, 0x13, 0x06, 0x07, 0x2a, 0x86, 0x48, 0xce, 0x3d, 0x02, 0x01, 0x06, 0x08, 0x2a, 0x86,
        0x48, 0xce, 0x3d, 0x03, 0x01, 0x07,
    ];
    let mut inner = version.to_vec();
    inner.extend_from_slice(&alg_id);
    inner.extend_from_slice(&asn1_wrap(4.0, &der));
    Ok(asn1_wrap(48.0, &inner))
}
