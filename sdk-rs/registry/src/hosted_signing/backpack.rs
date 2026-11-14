//! Device-owned Backpack Ed25519 graph. Only public payloads cross the wire.
use super::{Output, OutputKind};
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Operation, PayloadPart, SignError, SignReceipt, SignRequest,
    SignResult,
};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Slot {
    #[default]
    Primary,
    Secondary,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Descriptor {
    #[serde(default)]
    slot: Slot,
    payload: String,
}

pub(super) fn prepare(
    ids: &Identities,
    descriptor: String,
    receipt: SignReceipt,
    ws: bool,
) -> Result<(SignRequest, OutputKind), SignError> {
    let input: Descriptor =
        serde_json::from_str(&descriptor).map_err(|_| SignError::InvalidRequest)?;
    if input.payload.is_empty() {
        return Err(SignError::InvalidRequest);
    }
    let identity = match input.slot {
        Slot::Primary => &ids.primary,
        Slot::Secondary => &ids.secondary,
    }
    .as_ref()
    .ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    receipt.validate()?;
    let work = SignRequest {
        receipt,
        context: Context {
            venue: "backpack".into(),
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
    Ok((work, OutputKind::Backpack { ws }))
}

pub(super) fn finish(ws: bool, result: &SignResult) -> Result<Output, SignError> {
    if !result.headers.is_empty() || result.values.len() != 2 || result.values[1].len() != 64 {
        return Err(SignError::InvalidResult);
    }
    let credential =
        std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
    if credential.is_empty() || credential.contains(['\r', '\n', '\0']) {
        return Err(SignError::InvalidResult);
    }
    let signature = safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64);
    if ws {
        #[derive(Serialize)]
        struct Batch<'a> {
            credential: &'a str,
            signature: String,
        }
        return Ok(Output {
            credential: None,
            headers: None,
            signature: Some(
                serde_json::to_string(&Batch {
                    credential,
                    signature,
                })
                .map_err(|_| SignError::InvalidResult)?,
            ),
        });
    }
    Ok(Output {
        credential: Some(credential.into()),
        signature: Some(signature),
        headers: None,
    })
}
