//! Device-owned Futures SHA256/HMAC-SHA512 graph; no refs cross the remote wire.
use super::{Output, OutputKind};
use safe_cex_engine::signing::{Identities, SignError, SignReceipt, SignRequest, SignResult};
use serde::{Deserialize, Serialize};

#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
enum Slot {
    #[default]
    Primary,
    Secondary,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    #[serde(default)]
    slot: Slot,
    payload: String,
}

pub(super) fn prepare(
    ids: &Identities,
    descriptor: &str,
    receipt: SignReceipt,
    batch: bool,
) -> Result<(SignRequest, OutputKind), SignError> {
    let input: Descriptor =
        serde_json::from_str(descriptor).map_err(|_| SignError::InvalidRequest)?;
    if input.payload.len() > 1_048_576 {
        return Err(SignError::InvalidRequest);
    }
    let identity = match input.slot {
        Slot::Primary => &ids.primary,
        Slot::Secondary => &ids.secondary,
    }
    .as_ref()
    .ok_or(SignError::Unavailable)?;
    let work = safe_cex_venue_kraken::sign::dispatch::canonical_work(
        identity,
        input.payload.into_bytes(),
        receipt,
    )?;
    work.validate()?;
    Ok((work, OutputKind::Kraken { batch }))
}

pub(super) fn finish(batch: bool, result: &SignResult) -> Result<Output, SignError> {
    if !result.headers.is_empty()
        || result.values.len() != 3
        || result.values[1].len() != 32
        || result.values[2].len() != 64
    {
        return Err(SignError::InvalidResult);
    }
    let credential =
        std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
    if credential.is_empty() || credential.contains(['\r', '\n', '\0']) {
        return Err(SignError::InvalidResult);
    }
    let signature = safe_cex_crypto::encode(&result.values[2], safe_cex_crypto::Encoding::Base64);
    let signature = if batch {
        #[derive(Serialize)]
        struct Batch<'a> {
            credential: &'a str,
            signature: String,
        }
        serde_json::to_string(&Batch {
            credential,
            signature,
        })
        .map_err(|_| SignError::InvalidResult)?
    } else {
        signature
    };
    Ok(Output {
        credential: (!batch).then(|| credential.into()),
        signature: Some(signature),
        headers: None,
    })
}
