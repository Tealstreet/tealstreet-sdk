//! Orderly device work selects only the primary identity owned by this engine.
//! This codec does not admit hosted construction or install a remote provider.
use super::{Output, OutputKind};
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use serde::{Deserialize, Serialize};

const MAX_DESCRIPTOR_BYTES: usize = 1024 * 1024;
const MAX_STAMP_MS: f64 = 9_007_199_254_740_991.;

#[derive(Deserialize)]
enum Version {
    #[serde(rename = "orderly/1")]
    Orderly,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Slot {
    Primary,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Kind {
    Http,
    WsAuth,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    version: Version,
    venue: String,
    slot: Slot,
    kind: Kind,
    stamp_ms: f64,
    payload: String,
}

pub(super) fn family(venue: &str) -> bool {
    matches!(venue, "orderly" | "kodiakfi" | "mode" | "what" | "woofi")
}

pub(super) fn prepare(
    venue: &str,
    ids: &Identities,
    descriptor: String,
    receipt: Option<SignReceipt>,
) -> Result<(SignRequest, OutputKind), SignError> {
    if descriptor.len() > MAX_DESCRIPTOR_BYTES || !descriptor.trim_start().starts_with('{') {
        return Err(SignError::InvalidRequest);
    }
    let input: Descriptor =
        serde_json::from_str(&descriptor).map_err(|_| SignError::InvalidRequest)?;
    let Version::Orderly = input.version;
    let Slot::Primary = input.slot;
    if input.venue != venue
        || !family(venue)
        || !input.stamp_ms.is_finite()
        || input.stamp_ms.is_sign_negative()
        || !(0. ..=MAX_STAMP_MS).contains(&input.stamp_ms)
        || input.stamp_ms.fract() != 0.
    {
        return Err(SignError::InvalidRequest);
    }
    let timestamp = format!("{:.0}", input.stamp_ms);
    let canonical = match input.kind {
        Kind::WsAuth => input.payload == timestamp,
        Kind::Http => input
            .payload
            .strip_prefix(&timestamp)
            .is_some_and(|request| {
                ["GET/", "POST/", "PUT/", "DELETE/"]
                    .iter()
                    .any(|prefix| request.starts_with(prefix))
            }),
    };
    if !canonical {
        return Err(SignError::InvalidRequest);
    }
    let ws = receipt.is_none();
    let receipt = match receipt {
        Some(receipt) => receipt,
        None => SignReceipt {
            stamp_ms: Some(input.stamp_ms),
            ..Default::default()
        },
    };
    receipt.validate()?;
    if receipt.stamp_ms != Some(input.stamp_ms)
        || receipt.nonce.is_some()
        || receipt.expiry_ms.is_some()
    {
        return Err(SignError::InvalidRequest);
    }
    let identity = ids.primary.as_ref().ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let work = SignRequest {
        receipt,
        context: Context {
            venue: venue.into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                algorithm: Algorithm::Ed25519,
                key: identity.signing_ref()?.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: input.payload.into_bytes(),
                }],
            },
        ],
    };
    work.validate()?;
    Ok((work, OutputKind::Orderly { ws }))
}

pub(super) fn finish(ws: bool, result: &SignResult) -> Result<Output, SignError> {
    let [credential, signature] = result.values.as_slice() else {
        return Err(SignError::InvalidResult);
    };
    if !result.headers.is_empty() || signature.len() != 64 {
        return Err(SignError::InvalidResult);
    }
    let credential = std::str::from_utf8(credential).map_err(|_| SignError::InvalidResult)?;
    let key = credential
        .strip_prefix("ed25519:")
        .filter(|key| (32..=44).contains(&key.len()))
        .ok_or(SignError::InvalidResult)?;
    let public = bs58::decode(key)
        .into_vec()
        .map_err(|_| SignError::InvalidResult)?;
    if public.len() != 32 {
        return Err(SignError::InvalidResult);
    }
    let signature = safe_cex_crypto::encode(signature, safe_cex_crypto::Encoding::Base64)
        .replace('+', "-")
        .replace('/', "_");
    if ws {
        #[derive(Serialize)]
        struct Batch<'a> {
            credential: &'a str,
            signature: String,
        }
        return Ok(Output {
            credential: None,
            signature: Some(
                serde_json::to_string(&Batch {
                    credential,
                    signature,
                })
                .map_err(|_| SignError::InvalidResult)?,
            ),
            headers: None,
        });
    }
    Ok(Output {
        credential: Some(credential.into()),
        signature: Some(signature),
        headers: None,
    })
}
