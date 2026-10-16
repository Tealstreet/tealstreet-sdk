//! Pure request and error helpers from asterdex.api.ts.
use safe_cex_num::{JsCoercionError, js_own_keys, js_string, js_truthy, normalize_object_order};
use serde_json::{Map, Value};

const PUBLIC_FAPI_PREFIXES: &[&str] = &[
    "/fapi/v1/ticker",
    "/fapi/v1/klines",
    "/fapi/v1/depth",
    "/fapi/v1/trades",
    "/fapi/v1/aggTrades",
    "/fapi/v1/exchangeInfo",
    "/fapi/v1/ping",
    "/fapi/v1/time",
    "/fapi/v3/ticker",
    "/fapi/v3/klines",
    "/fapi/v3/depth",
    "/fapi/v3/trades",
    "/fapi/v3/aggTrades",
    "/fapi/v3/exchangeInfo",
    "/fapi/v3/ping",
    "/fapi/v3/time",
];

pub fn get_aster_dex_code_chain_id(testnet: bool) -> u32 {
    if testnet { 714 } else { 1666 }
}
pub fn get_aster_dex_aster_chain(testnet: bool) -> &'static str {
    if testnet { "Testnet" } else { "Mainnet" }
}

/// Sign AsterDEX's small EIP-712 `Message` payload using the shared Ethereum
/// implementation. The key normalization mirrors ethers' accepted bare-hex
/// input without creating a venue-local signer.
pub fn generate_aster_dex_code_message_signature(
    query_string: &str,
    signer_private_key: &str,
    testnet: Option<bool>,
) -> Result<String, safe_cex_exchange::runtime::ExchangeError> {
    let private_key = if signer_private_key.starts_with("0x") {
        signer_private_key.to_owned()
    } else {
        format!("0x{signer_private_key}")
    };
    let payload = serde_json::json!({
        "domain": {
            "name": "AsterSignTransaction",
            "version": "1",
            "chainId": get_aster_dex_code_chain_id(testnet.unwrap_or(false)),
            "verifyingContract": "0x0000000000000000000000000000000000000000"
        },
        "types": {
            "Message": [{"name": "msg", "type": "string"}]
        },
        "primaryType": "Message",
        "message": {"msg": query_string}
    });
    let result = safe_cex_crypto::eip712::sign_typed_data(&payload.to_string(), &private_key);
    let key_bytes = &private_key[2..];
    if key_bytes.len() != 64 || !key_bytes.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        let mut error =
            safe_cex_exchange::runtime::ExchangeError::new("Invalid Aster signer private key");
        error.name = "TypeError".into();
        return Err(error);
    }
    result.map_err(safe_cex_exchange::runtime::ExchangeError::new)
}

pub fn build_aster_dex_code_query_string(
    params: &Map<String, Value>,
) -> Result<String, JsCoercionError> {
    build_query_string(params)
}

pub fn build_query_string(params: &Map<String, Value>) -> Result<String, JsCoercionError> {
    let mut parts = Vec::new();
    for key in js_own_keys(params.keys().map(String::as_str)) {
        let value = &params[key];
        if value.is_null() || value.as_str() == Some("") {
            continue;
        }
        parts.push(format!("{key}={}", js_string(Some(value))?));
    }
    Ok(parts.join("&"))
}

const SIGNING_FIELDS: [&str; 6] = [
    "signature",
    "timestamp",
    "recvWindow",
    "user",
    "signer",
    "nonce",
];

/// ts: withoutSigningFields. A resend's params minus the previous attempt's signing fields.
pub fn without_signing_fields(params: &Value) -> Map<String, Value> {
    let mut entries = match params {
        Value::Object(map) => map.clone(),
        Value::Array(items) => items
            .iter()
            .enumerate()
            .map(|(index, item)| (index.to_string(), item.clone()))
            .collect(),
        _ => return Map::new(),
    };
    normalize_object_order(&mut entries);
    entries.retain(|key, _| !SIGNING_FIELDS.contains(&key.as_str()));
    entries
}

pub fn build_v3_signing_payload(
    params: &Map<String, Value>,
) -> Result<Map<String, Value>, JsCoercionError> {
    let mut keys: Vec<_> = params.keys().collect();
    keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    let mut result = Map::new();
    for key in keys {
        let value = &params[key];
        if value.is_null() || value.as_str() == Some("") {
            continue;
        }
        let text = js_string(Some(value))?;
        // ts-bug: buildV3SigningPayload assigns strings to Object.prototype.__proto__, which ignores them.
        if key != "__proto__" {
            result.insert(key.clone(), Value::String(text));
        }
    }
    normalize_object_order(&mut result);
    Ok(result)
}

pub fn strip_query_string(url: &str) -> &str {
    url.split('?').next().unwrap_or("")
}
pub fn is_public_endpoint(url: Option<&str>) -> bool {
    url.is_some_and(|u| {
        !u.is_empty()
            && PUBLIC_FAPI_PREFIXES
                .iter()
                .any(|prefix| strip_query_string(u).contains(prefix))
    })
}
pub fn is_aster_dex_code_signed_endpoint(url: Option<&str>, method: Option<&str>) -> bool {
    let Some(url) = url.filter(|s| !s.is_empty()) else {
        return false;
    };
    let path = strip_query_string(url);
    let method = method
        .filter(|s| !s.is_empty())
        .unwrap_or("get")
        .to_lowercase();
    (method == "post" && path.ends_with("/fapi/v3/order"))
        || (method == "get"
            && (path.ends_with("/fapi/v3/builder") || path.ends_with("/fapi/v3/agent")))
}
pub fn should_suppress_unknown_order_error(
    config: Option<&Value>,
    code: Option<&Value>,
    message: Option<&Value>,
) -> Result<bool, JsCoercionError> {
    if !js_truthy(config.and_then(|c| c.get("__suppressAsterUnknownOrderError"))) {
        return Ok(false);
    }
    if js_string(code)? != "-2011" {
        return Ok(false);
    }
    let empty = Value::String(String::new());
    let message = if js_truthy(message) {
        message
    } else {
        Some(&empty)
    };
    Ok(js_string(message)?.to_lowercase().contains("unknown order"))
}
pub fn get_aster_dex_error_detail(
    message: Option<&Value>,
    code: Option<&Value>,
) -> Result<String, JsCoercionError> {
    let fallback = Value::String("Unknown error".into());
    let raw = if let Some(Value::String(s)) = message {
        s.clone()
    } else {
        js_string(if js_truthy(message) {
            message
        } else {
            Some(&fallback)
        })?
    };
    if js_string(code)? == "-5025" || raw.to_lowercase().contains("cannot found builder config") {
        Ok("Aster builder attribution is not synced for this API key. Open your Aster API key settings, sync or enable the Tealstreet builder authorization, then reconnect the account and try again.".into())
    } else {
        Ok(raw)
    }
}

pub fn is_asterdex_read(request: &safe_cex_transport::RestRequest) -> bool {
    safe_cex_transport::retry::is_read_only_method(request)
        || strip_query_string(&request.url).ends_with("/listenKey")
}

pub async fn generate_v1_signature(query_string: &str, secret: &str) -> Result<String, String> {
    let options = safe_cex_crypto::SigningOptions {
        key: "",
        secret,
        payload_signer: None,
    };
    safe_cex_crypto::sign_payload(
        &options,
        safe_cex_types::ExchangeName::Asterdex,
        query_string,
        safe_cex_crypto::Encoding::Hex,
    )
    .await
    .map(|r| r.signature)
}

static LAST_V3_NONCE_BY_SIGNER: std::sync::Mutex<
    std::collections::BTreeMap<String, num_bigint::BigInt>,
> = std::sync::Mutex::new(std::collections::BTreeMap::new());

// ts: buildV3Nonce
// The caller supplies the venue virtual-clock reading; module state is shared across native executor threads.
pub fn build_v3_nonce(
    nonce_scope_key: &str,
    current_time_ms: f64,
) -> Result<String, safe_cex_num::JsBigIntError> {
    // Conversion throws before inspecting or mutating signer state, as in TS.
    let micros =
        safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::Number(current_time_ms))?
            * 1000;
    let mut state = LAST_V3_NONCE_BY_SIGNER
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    Ok({
        let previous = state.get(nonce_scope_key).cloned().unwrap_or_default();
        let next = if micros > previous {
            micros
        } else {
            previous + 1_u32
        };
        let result = next.to_string();
        state.insert(nonce_scope_key.to_owned(), next);
        result
    })
}

// ts: buildAsterDEXV3Nonce
pub fn build_aster_dex_v3_nonce(
    nonce_scope_key: &str,
    current_time_ms: f64,
) -> Result<String, safe_cex_num::JsBigIntError> {
    build_v3_nonce(nonce_scope_key, current_time_ms)
}
