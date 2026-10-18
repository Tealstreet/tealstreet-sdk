//! Signing primitives; behavioral reference: packages/safe-cex/src/exchanges/CLAUDE.md.
use base64::{Engine, engine::general_purpose::STANDARD};
use hmac::{Hmac, Mac};
use safe_cex_types::ExchangeName;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256, Sha512};
use std::{collections::BTreeMap, future::Future, pin::Pin};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Encoding {
    Hex,
    Base64,
}

#[derive(Deserialize)]
#[serde(remote = "Encoding", rename_all = "lowercase")]
enum EncodingWire {
    Hex,
    Base64,
}
impl<'de> Deserialize<'de> for Encoding {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        EncodingWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
    }
}

fn encode(bytes: &[u8], encoding: Encoding) -> String {
    match encoding {
        Encoding::Hex => bytes.iter().map(|byte| format!("{byte:02x}")).collect(),
        Encoding::Base64 => STANDARD.encode(bytes),
    }
}

pub fn hmac_sha256(key: &[u8], payload: &[u8], encoding: Encoding) -> String {
    let mut mac = Hmac::<Sha256>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload);
    encode(&mac.finalize().into_bytes(), encoding)
}

pub fn sha256(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

#[derive(Debug, PartialEq, Eq)]
pub enum PayloadScheme {
    HmacSha256 { encoding: Encoding },
}

#[derive(Debug)]
pub struct PayloadSignerRequest<'a> {
    pub exchange_name: ExchangeName,
    pub key: &'a str,
    pub payload: &'a str,
    pub scheme: PayloadScheme,
}

#[derive(Debug, PartialEq, Eq)]
pub struct PayloadSignerResult {
    pub signature: String,
    pub headers: Option<BTreeMap<String, String>>,
}

// No Send bound: browser-hosted signers may retain single-threaded host handles.
pub trait PayloadSigner {
    fn sign<'a>(
        &'a self,
        request: PayloadSignerRequest<'a>,
    ) -> Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + 'a>>;
}

pub struct SigningOptions<'a> {
    pub key: &'a str,
    pub secret: &'a str,
    pub payload_signer: Option<&'a dyn PayloadSigner>,
}

pub fn can_sign_payload(options: &SigningOptions<'_>) -> bool {
    !options.secret.is_empty() || options.payload_signer.is_some()
}

pub async fn sign_payload(
    options: &SigningOptions<'_>,
    exchange_name: ExchangeName,
    payload: &str,
    encoding: Encoding,
) -> Result<PayloadSignerResult, String> {
    if let Some(signer) = options.payload_signer {
        return signer
            .sign(PayloadSignerRequest {
                exchange_name,
                key: options.key,
                payload,
                scheme: PayloadScheme::HmacSha256 { encoding },
            })
            .await;
    }
    Ok(PayloadSignerResult {
        signature: hmac_sha256(options.secret.as_bytes(), payload.as_bytes(), encoding),
        headers: None,
    })
}

#[cfg(test)]
mod payload_tests;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum WsOutputFormat {
    Hex,
    Base64,
    HexBase64,
    KrakenChallenge,
    ApexomniBase64,
    BitunixDoubleSha256,
    CoinbaseJwt,
}

#[derive(Deserialize)]
#[serde(remote = "WsOutputFormat", rename_all = "kebab-case")]
enum WsOutputFormatWire {
    Hex,
    Base64,
    HexBase64,
    KrakenChallenge,
    ApexomniBase64,
    BitunixDoubleSha256,
    CoinbaseJwt,
}
impl<'de> Deserialize<'de> for WsOutputFormat {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        WsOutputFormatWire::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
            value,
        ))
    }
}

pub use safe_cex_types::ws_auth::{WsAuth, WsAuthRequest, WsAuthResponse};

// arbitrary_precision can retain numeric tokens beyond f64's finite range.
fn js_number(value: &serde_json::Number) -> f64 {
    value
        .as_f64()
        .unwrap_or_else(|| value.to_string().parse::<f64>().unwrap_or(f64::NAN))
}

fn json_is_truthy(value: &Value) -> bool {
    match value {
        Value::Null => false,
        Value::Bool(value) => *value,
        Value::Number(value) => {
            let number = js_number(value);
            !number.is_nan() && number != 0.0
        }
        Value::String(value) => !value.is_empty(),
        Value::Array(_) | Value::Object(_) => true,
    }
}

fn js_string(value: &Value) -> String {
    match value {
        Value::Null => "null".into(),
        Value::Bool(value) => value.to_string(),
        Value::Number(value) => ryu_js::Buffer::new().format(js_number(value)).to_owned(),
        Value::String(value) => value.clone(),
        Value::Array(values) => values
            .iter()
            .map(|value| {
                if value.is_null() {
                    String::new()
                } else {
                    js_string(value)
                }
            })
            .collect::<Vec<_>>()
            .join(","),
        Value::Object(_) => "[object Object]".into(),
    }
}

fn ws_response(id: &Option<Value>, result: Result<String, &str>) -> WsAuthResponse {
    match result {
        Ok(signature) => WsAuthResponse {
            id: id.clone(),
            ws_auth_signature: Some(signature),
            error: None,
        },
        Err(error) => WsAuthResponse {
            id: id.clone(),
            ws_auth_signature: None,
            error: Some(error.into()),
        },
    }
}

// ts-bug: BaseExchange.signWsAuth rejects truthy non-string payloads without settling auth.
// Keep the correlated refusal; the canonical type move preserves this existing choice.
pub fn sign_ws_auth(request: &WsAuthRequest, secret: &str) -> WsAuthResponse {
    let id = &request.id;
    let payload = request
        .ws_auth
        .as_ref()
        .and_then(|auth| Some((auth.to_sign.as_deref()?, auth.output_format.as_ref()?)));
    let Some((to_sign, output_format)) =
        payload.filter(|(payload, format)| !payload.is_empty() && json_is_truthy(format))
    else {
        return ws_response(id, Err("Missing wsAuth payload for WS auth signing"));
    };
    if secret.is_empty() {
        return ws_response(id, Err("No secret available for WS auth signing"));
    }
    // Only actual strings may select a signer; coercion is for errors alone.
    let format = output_format.as_str().and_then(|format| {
        WsOutputFormat::deserialize(
            serde::de::value::StrDeserializer::<serde::de::value::Error>::new(format),
        )
        .ok()
    });
    let Some(output_format_enum) = format else {
        return ws_response(
            id,
            Err(&format!(
                "Unsupported WS auth outputFormat: {}",
                js_string(output_format)
            )),
        );
    };
    let payload = to_sign.as_bytes();
    // Rely on Rust exhaustiveness: a new format MUST require a named signer arm.
    // Never add a wildcard; it restores the silent wrong-signature fallback.
    let signature = match output_format_enum {
        WsOutputFormat::Hex => hmac_sha256(secret.as_bytes(), payload, Encoding::Hex),
        WsOutputFormat::Base64 => hmac_sha256(secret.as_bytes(), payload, Encoding::Base64),
        WsOutputFormat::HexBase64 => {
            STANDARD.encode(hmac_sha256(secret.as_bytes(), payload, Encoding::Hex))
        }
        WsOutputFormat::ApexomniBase64 => hmac_sha256(
            STANDARD.encode(secret).as_bytes(),
            payload,
            Encoding::Base64,
        ),
        WsOutputFormat::BitunixDoubleSha256 => {
            let inner = encode(&Sha256::digest(payload), Encoding::Hex);
            encode(
                &Sha256::digest(format!("{inner}{secret}").as_bytes()),
                Encoding::Hex,
            )
        }
        WsOutputFormat::KrakenChallenge => {
            return ws_response(
                id,
                Err("kraken-challenge requires the KrakenExchange signWsAuth override"),
            );
        }
        WsOutputFormat::CoinbaseJwt => {
            return ws_response(
                id,
                Err("coinbase-jwt requires the CoinbaseExchange signWsAuth override"),
            );
        }
    };
    ws_response(id, Ok(signature))
}

// Node-compatible Buffer decoding; browser equivalence is bounded to ASCII secrets.
// Malformed non-ASCII input differs in TS browser/Node hosts (see VERIFICATION.md).
/// Decode using Node Buffer base64 rules, including ignored characters and trailing bits.
pub fn node_base64_decode(input: &str) -> Vec<u8> {
    let mut output = Vec::new();
    let mut accumulator = 0u32;
    let mut bits = 0u32;
    // Node truncates UTF-16 code units, even for malformed non-ASCII secrets.
    for byte in input.encode_utf16().map(|unit| unit as u8) {
        let value = match byte {
            b'A'..=b'Z' => byte - b'A',
            b'a'..=b'z' => byte - b'a' + 26,
            b'0'..=b'9' => byte - b'0' + 52,
            b'+' | b'-' => 62,
            b'/' | b'_' => 63,
            b'=' => break,
            _ => continue,
        };
        accumulator = (accumulator << 6) | u32::from(value);
        bits += 6;
        if bits >= 8 {
            bits -= 8;
            output.push((accumulator >> bits) as u8);
            accumulator &= (1 << bits) - 1;
        }
    }
    output
}

// ts-bug: KrakenExchange.signWsAuth has the same non-string payload caller hang.
pub fn sign_kraken_ws_auth(request: &WsAuthRequest, secret: &str) -> WsAuthResponse {
    let id = &request.id;
    let challenge = request
        .ws_auth
        .as_ref()
        .and_then(|auth| auth.to_sign.as_deref());
    let Some(challenge) = challenge.filter(|challenge| !challenge.is_empty()) else {
        return ws_response(id, Err("Missing challenge to sign"));
    };
    if secret.is_empty() {
        return ws_response(id, Err("No secret available for WS auth signing"));
    }
    // Like the TS override, this signs the challenge regardless of output_format.
    let hash = Sha256::digest(challenge.as_bytes());
    ws_response(
        id,
        Ok(hmac_sha512(
            &node_base64_decode(secret),
            &hash,
            Encoding::Base64,
        )),
    )
}

#[cfg(test)]
mod ws_tests;
pub(crate) fn hmac_sha512(key: &[u8], payload: &[u8], encoding: Encoding) -> String {
    let mut mac = Hmac::<Sha512>::new_from_slice(key).expect("HMAC accepts any key length");
    mac.update(payload);
    encode(&mac.finalize().into_bytes(), encoding)
}

// P5a primitives only: callers own venue preimages and transport.
pub mod ed25519;
pub mod eip712;
pub mod es256;
pub mod lighter;
pub mod msgpack;
pub mod poseidon_types;
pub mod starknet;
pub mod zklink;

/// Ethereum ABI, address and recoverable-signature primitives.
pub mod ethereum;

pub mod uuid;
pub mod wallet_utils;

pub mod poseidon_example;
pub mod poseidon_loaders;
