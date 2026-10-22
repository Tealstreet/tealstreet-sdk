//! Compatibility inputs contain canonical bytes, never caller-chosen key refs.
//! The SDK retains its existing serializers; family recipes build atomic work.
use safe_cex_engine::signing::{
    Context, Identities, Operation, SignError, SignReceipt, SignRequest, SignResult,
};
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Deserialize)]
pub enum OutputFormat {
    #[serde(rename = "hex")]
    Hex,
    #[serde(rename = "base64")]
    Base64,
    #[serde(rename = "hex-base64")]
    Base64Hex,
}

#[derive(Deserialize)]
#[serde(
    tag = "kind",
    rename_all = "camelCase",
    rename_all_fields = "camelCase",
    deny_unknown_fields
)]
pub enum Input {
    Credential {},
    Http {
        payload: String,
        stamp_ms: f64,
        revision: String,
    },
    WsAuth {
        to_sign: String,
        output_format: OutputFormat,
    },
}

/// Formatting metadata remains bound to its one capture token.
#[derive(Clone, Copy)]
pub enum OutputKind {
    Credential,
    Http,
    WsAuth(OutputFormat),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Output {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub credential: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub signature: Option<String>,
}

pub fn prepare(
    venue: &str,
    identities: &Identities,
    input: Input,
) -> Result<(SignRequest, OutputKind), SignError> {
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let context = Context {
        venue: venue.into(),
        method: "HOSTED".into(),
        ..Default::default()
    };
    let binance = cfg!(feature = "binance")
        && matches!(
            venue,
            "binance" | "binancepm" | "binanceinverse" | "binancespot"
        );
    let bybit = cfg!(feature = "bybit")
        && matches!(
            venue,
            "bybit" | "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader"
        );
    if !binance && !bybit {
        return Err(SignError::Unsupported);
    }
    match input {
        Input::Credential {} => Ok((
            SignRequest {
                receipt: SignReceipt::default(),
                context,
                operations: vec![Operation::Present {
                    key: identity.credential.clone(),
                }],
            },
            OutputKind::Credential,
        )),
        Input::Http {
            payload,
            stamp_ms,
            revision: _,
        } => {
            let receipt = SignReceipt {
                stamp_ms: Some(stamp_ms),
                expiry_ms: None,
            };
            receipt.validate()?;
            Ok((
                safe_cex_engine::signing::hmac_sha256_work(
                    identity,
                    vec![safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: payload.into_bytes(),
                    }],
                    context,
                    receipt,
                ),
                OutputKind::Http,
            ))
        }
        Input::WsAuth {
            to_sign,
            output_format,
        } => {
            if to_sign.is_empty() {
                return Err(SignError::InvalidRequest);
            }
            Ok((
                safe_cex_engine::signing::hmac_sha256_work(
                    identity,
                    vec![safe_cex_engine::signing::PayloadPart::Bytes {
                        bytes: to_sign.into_bytes(),
                    }],
                    context,
                    SignReceipt::default(),
                ),
                OutputKind::WsAuth(output_format),
            ))
        }
    }
}

pub fn finish(kind: OutputKind, result: &SignResult) -> Result<Output, SignError> {
    let key = result.values.first().ok_or(SignError::InvalidResult)?;
    let key = std::str::from_utf8(key).map_err(|_| SignError::InvalidResult)?;
    if key.is_empty() || key.contains(['\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    if matches!(kind, OutputKind::Credential) {
        return Ok(Output {
            credential: Some(key.into()),
            signature: None,
        });
    }
    let mac = result
        .values
        .get(1)
        .filter(|mac| mac.len() == 32)
        .ok_or(SignError::InvalidResult)?;
    let signature = match kind {
        OutputKind::Credential => unreachable!(),
        OutputKind::Http | OutputKind::WsAuth(OutputFormat::Hex) => safe_cex_crypto::hex(mac),
        OutputKind::WsAuth(OutputFormat::Base64) => {
            safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64)
        }
        OutputKind::WsAuth(OutputFormat::Base64Hex) => safe_cex_crypto::encode(
            safe_cex_crypto::hex(mac).as_bytes(),
            safe_cex_crypto::Encoding::Base64,
        ),
    };
    Ok(Output {
        credential: matches!(kind, OutputKind::Http).then(|| key.into()),
        signature: Some(signature),
    })
}
