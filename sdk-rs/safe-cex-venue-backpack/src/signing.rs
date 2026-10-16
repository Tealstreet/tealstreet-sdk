//! Pure signing preimage helpers from backpack.signing.ts.
use safe_cex_num::{JsJsonValue, js_number_string, js_string};
use serde_json::Value;

// Lower only the String(value) observation into the shared coercion helper.
// Numbers/BigInts become their exact JS spellings, never lossy JSON numbers.
// Object values are unobserved by String(object); own toString still matters.
pub(crate) fn string_input(value: &JsJsonValue) -> Value {
    match value {
        JsJsonValue::Undefined | JsJsonValue::Null => Value::Null,
        JsJsonValue::Bool(v) => Value::Bool(*v),
        JsJsonValue::Number(v) => Value::String(js_number_string(*v)),
        JsJsonValue::BigInt(v) => Value::String(v.to_string()),
        JsJsonValue::String(v) => Value::String(v.clone()),
        JsJsonValue::Array(v) => Value::Array(v.iter().map(string_input).collect()),
        JsJsonValue::Object(v) => Value::Object(
            v.iter()
                .map(|(key, _)| (key.clone(), Value::Null))
                .collect(),
        ),
    }
}

fn encode_component(value: &str) -> String {
    let mut output = String::new();
    const HEX: &[u8] = b"0123456789ABCDEF";
    for byte in value.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            output.push(char::from(byte));
        } else {
            output.push('%');
            output.push(char::from(HEX[usize::from(byte >> 4)]));
            output.push(char::from(HEX[usize::from(byte & 15)]));
        }
    }
    output
}

// ts: buildSortedQueryString
pub fn build_sorted_query_string(params: &JsJsonValue) -> Result<String, String> {
    let entries = match params {
        JsJsonValue::Object(v) => v.clone(),
        JsJsonValue::Array(v) => v
            .iter()
            .enumerate()
            .map(|(i, v)| (i.to_string(), v.clone()))
            .collect(),
        JsJsonValue::String(v) => v
            .encode_utf16()
            .enumerate()
            .map(|(i, unit)| {
                String::from_utf16(&[unit])
                    .map(|s| (i.to_string(), JsJsonValue::String(s)))
                    .map_err(|_| "URIError: URI malformed".to_owned())
            })
            .collect::<Result<Vec<_>, _>>()?,
        JsJsonValue::Null | JsJsonValue::Undefined => {
            return Err("TypeError: Cannot convert undefined or null to object".into());
        }
        JsJsonValue::Bool(_) | JsJsonValue::Number(_) | JsJsonValue::BigInt(_) => Vec::new(),
    };
    let mut keys: Vec<_> = entries.iter().map(|(key, _)| key.as_str()).collect();
    keys.sort_by(|a, b| a.encode_utf16().cmp(b.encode_utf16()));
    keys.dedup();
    let mut parts = Vec::new();
    for key in keys {
        let value = &entries.iter().rev().find(|(k, _)| k == key).unwrap().1;
        if !matches!(value, JsJsonValue::Null | JsJsonValue::Undefined) {
            let text = js_string(Some(&string_input(value))).map_err(|e| e.to_string())?;
            parts.push(format!("{key}={}", encode_component(&text)));
        }
    }
    Ok(parts.join("&"))
}

// ts: buildSigningPayload
pub fn build_signing_payload(
    instruction: &str,
    params: &JsJsonValue,
    timestamp: f64,
    window: f64,
) -> Result<String, String> {
    let prefix = if let JsJsonValue::Array(rows) = params {
        if instruction != "orderExecute" {
            return Err(format!(
                "Error: Backpack array signing is only defined for orderExecute, received {instruction}"
            ));
        }
        if rows.is_empty() {
            return Err("Error: Backpack batch signing requires at least one order".into());
        }
        let mut row_payloads = Vec::new();
        for row in rows {
            if !matches!(row, JsJsonValue::Object(_)) {
                return Err("Error: Backpack batch signing requires object rows".into());
            }
            let query = build_sorted_query_string(row)?;
            row_payloads.push(format!(
                "instruction={instruction}{}",
                if query.is_empty() {
                    String::new()
                } else {
                    format!("&{query}")
                }
            ));
        }
        row_payloads.join("&")
    } else {
        let query = build_sorted_query_string(params)?;
        format!(
            "instruction={instruction}{}",
            if query.is_empty() {
                String::new()
            } else {
                format!("&{query}")
            }
        )
    };
    Ok(format!(
        "{prefix}&timestamp={}&window={}",
        js_number_string(timestamp),
        js_number_string(window)
    ))
}

// Buffer.from(text, 'base64') accepts URL-safe letters, skips nonalphabet
// UTF-16 low bytes, stops at padding, and ignores an incomplete final sextet.
// The base64 crate performs the actual byte decoding after this caller policy.
fn decode_node_base64(input: &str) -> Vec<u8> {
    use base64::{
        Engine, alphabet,
        engine::{DecodePaddingMode, GeneralPurpose, GeneralPurposeConfig},
    };
    let mut normalized: Vec<u8> = input
        .encode_utf16()
        .map(|unit| unit as u8)
        .take_while(|byte| *byte != b'=')
        .filter_map(|byte| match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'+' | b'/' => Some(byte),
            b'-' => Some(b'+'),
            b'_' => Some(b'/'),
            _ => None,
        })
        .collect();
    if normalized.len() % 4 == 1 {
        normalized.pop();
    }
    let engine = GeneralPurpose::new(
        &alphabet::STANDARD,
        GeneralPurposeConfig::new()
            .with_decode_padding_mode(DecodePaddingMode::Indifferent)
            .with_decode_allow_trailing_bits(true),
    );
    engine
        .decode(normalized)
        .expect("normalized Node base64 alphabet and length")
}

// ts: signRequest
pub fn sign_request(
    instruction: &str,
    params: &JsJsonValue,
    timestamp: f64,
    window: f64,
    private_key_base64: &str,
) -> std::future::Ready<Result<String, String>> {
    // Source async functions execute their pre-await work when called.
    let result = (|| {
        use base64::{Engine, engine::general_purpose::STANDARD};
        let payload = build_signing_payload(instruction, params, timestamp, window)?;
        let message = payload.as_bytes();
        let private_key = decode_node_base64(private_key_base64);
        if private_key.len() != 32 {
            return Err(format!(
                "RangeError: \"secretKey\" expected Uint8Array of length 32, got length={}",
                private_key.len()
            ));
        }
        let signature = safe_cex_crypto::ed25519::sign(message, &private_key)
            .map_err(|error| format!("Error: {error}"))?;
        Ok(STANDARD.encode(signature))
    })();
    std::future::ready(result)
}

// ts: generateAuthHeaders
pub fn generate_auth_headers<F: FnOnce() -> f64>(
    instruction: &str,
    params: &JsJsonValue,
    api_key: &str,
    secret: &str,
    window: Option<f64>,
    now_ms: F,
) -> impl std::future::Future<Output = Result<Vec<(String, String)>, String>> + use<F> {
    let window = window.unwrap_or(5_000.0);
    let timestamp = now_ms();
    let signature = sign_request(instruction, params, timestamp, window, secret);
    let api_key = api_key.to_owned();
    async move {
        let signature = signature.await?;
        Ok(vec![
            ("X-API-Key".into(), api_key),
            ("X-Signature".into(), signature),
            ("X-Timestamp".into(), js_number_string(timestamp)),
            ("X-Window".into(), js_number_string(window)),
            ("Content-Type".into(), "application/json".into()),
        ])
    }
}

// ts: verifySignature
pub fn verify_signature(
    instruction: &str,
    params: &JsJsonValue,
    timestamp: f64,
    window: f64,
    signature: &str,
    public_key_base64: &str,
) -> std::future::Ready<bool> {
    // Source catches payload, decoding and verification failures to false.
    // Shared verification retains its explicitly chosen strict-key policy.
    let result = (|| {
        let payload = build_signing_payload(instruction, params, timestamp, window)?;
        let public_key = decode_node_base64(public_key_base64);
        let signature = decode_node_base64(signature);
        safe_cex_crypto::ed25519::verify(payload.as_bytes(), &public_key, &signature)
    })();
    std::future::ready(result.unwrap_or(false))
}

pub type SigningRecord<V> =
    std::rc::Rc<std::cell::RefCell<Vec<(crate::public_control::JsString, V)>>>;
// ts: BackpackSigningParams
pub enum BackpackSigningParams<V> {
    Object(SigningRecord<V>),
    Batch(std::rc::Rc<std::cell::RefCell<Vec<SigningRecord<V>>>>),
}
impl<V> BackpackSigningParams<V> {
    pub fn object(record: SigningRecord<V>) -> Self {
        Self::Object(record)
    }
    pub fn batch(rows: std::rc::Rc<std::cell::RefCell<Vec<SigningRecord<V>>>>) -> Self {
        Self::Batch(rows)
    }
}
// ts: generateWsAuthSignature
pub fn generate_ws_auth_signature<
    V,
    E,
    F: std::future::Future<Output = Result<crate::public_control::JsString, E>>,
>(
    api_key: crate::public_control::JsString,
    secret: crate::public_control::JsString,
    window: Option<f64>,
    now_ms: impl FnOnce() -> Result<f64, E>,
    sign: impl FnOnce(
        &str,
        BackpackSigningParams<V>,
        f64,
        f64,
        crate::public_control::JsString,
    ) -> Result<F, E>,
) -> impl std::future::Future<Output = Result<[crate::public_control::JsString; 4], E>> {
    let window = window.unwrap_or(5000.0);
    let start = now_ms().and_then(|timestamp| {
        sign(
            "subscribe",
            BackpackSigningParams::object(std::rc::Rc::new(std::cell::RefCell::new(Vec::new()))),
            timestamp,
            window,
            secret,
        )
        .map(|signature| (timestamp, signature))
    });
    async move {
        let (timestamp, signature) = start?;
        let signature = signature.await?;
        Ok([
            api_key,
            signature,
            js_number_string(timestamp).encode_utf16().collect(),
            js_number_string(window).encode_utf16().collect(),
        ])
    }
}

// ts: sha512ForEd25519
pub fn sha512_for_ed25519(message: &[u8]) -> Vec<u8> {
    use sha2::Digest;
    sha2::Sha512::digest(message).to_vec()
}
