//! Device-owned zkLink contracts followed by the API HMAC that embeds them.
use super::{Output, OutputKind};
use safe_cex_engine::signing::{
    Algorithm, Context, Encoding, ExtraKey, Identities, Operation, PayloadPart, SignError,
    SignReceipt, SignRequest, SignResult,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Part {
    bytes: Option<Vec<u8>>,
    contract: Option<usize>,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    contracts: Vec<Vec<u8>>,
    payload: Vec<Part>,
}

pub(super) fn prepare(
    ids: &Identities,
    descriptor: String,
    receipt: SignReceipt,
    ws: bool,
) -> Result<(SignRequest, OutputKind), SignError> {
    let input: Descriptor =
        serde_json::from_str(&descriptor).map_err(|_| SignError::InvalidRequest)?;
    let count = input.contracts.len();
    if count > 29 || input.payload.is_empty() || input.payload.len() > 256 || (!ws && count != 0) {
        return Err(SignError::InvalidRequest);
    }
    let identity = ids.primary.as_ref().ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let mut operations = Vec::with_capacity(count + 3);
    for bytes in input.contracts {
        // The only zkLink operation exposed here is a 36-byte contract order;
        // transfers, withdrawals and arbitrary signing blobs are not admitted.
        if bytes.len() != 36
            || bytes[0] != 0xfe
            || bytes[1..5] == [0; 4]
            || bytes[5] != 0
            || bytes[12] > 1
            || bytes[35] != 0
        {
            return Err(SignError::InvalidRequest);
        }
        operations.push(Operation::Sign {
            algorithm: Algorithm::ZkLinkContract,
            key: identity
                .extra
                .get(&ExtraKey::ZklinkSeed)
                .ok_or(SignError::Unavailable)?
                .clone(),
            payload: vec![PayloadPart::Bytes { bytes }],
        });
    }
    let mut seen = vec![false; count];
    let mut length = 0;
    let mut payload = Vec::with_capacity(input.payload.len());
    for part in input.payload {
        payload.push(match (part.bytes, part.contract) {
            (Some(bytes), None) => {
                length += bytes.len();
                if length > 1_048_576 {
                    return Err(SignError::InvalidRequest);
                }
                PayloadPart::Bytes { bytes }
            }
            (None, Some(index)) if index < count && !seen[index] => {
                seen[index] = true;
                PayloadPart::Result {
                    index,
                    encoding: Encoding::Hex,
                }
            }
            _ => return Err(SignError::InvalidRequest),
        });
    }
    if seen.contains(&false) || length == 0 {
        return Err(SignError::InvalidRequest);
    }
    operations.extend([
        Operation::Present {
            key: identity.credential.clone(),
        },
        Operation::Present {
            key: identity
                .passphrase
                .as_ref()
                .ok_or(SignError::Unavailable)?
                .clone(),
        },
        Operation::Sign {
            algorithm: Algorithm::HmacSha256,
            key: identity.signing_ref()?.clone(),
            payload,
        },
    ]);
    let work = SignRequest {
        receipt,
        operations,
        context: Context {
            venue: "apexomni".into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
    };
    work.validate()?;
    Ok((
        work,
        OutputKind::ApexOmni {
            contracts: count,
            ws,
        },
    ))
}

pub(super) fn finish(count: usize, ws: bool, result: &SignResult) -> Result<Output, SignError> {
    if count > 29
        || !result.headers.is_empty()
        || result.values.len() != count + 3
        || result.values[..count].iter().any(|value| value.len() != 64)
        || result.values[count + 2].len() != 32
    {
        return Err(SignError::InvalidResult);
    }
    let presented = |index: usize| {
        std::str::from_utf8(result.values[index].as_slice())
            .ok()
            .filter(|text| !text.is_empty() && !text.contains(['\r', '\n', '\0']))
            .ok_or(SignError::InvalidResult)
    };
    let credential = presented(count)?;
    let passphrase = presented(count + 1)?;
    let signature =
        safe_cex_crypto::encode(&result.values[count + 2], safe_cex_crypto::Encoding::Base64);
    if ws {
        #[derive(Serialize)]
        struct Batch<'a> {
            credential: &'a str,
            contracts: Vec<String>,
            signature: String,
        }
        let contracts = result.values[..count]
            .iter()
            .map(|bytes| safe_cex_crypto::hex(bytes))
            .collect();
        return Ok(Output {
            credential: None,
            headers: None,
            signature: Some(
                serde_json::to_string(&Batch {
                    credential,
                    contracts,
                    signature,
                })
                .map_err(|_| SignError::InvalidResult)?,
            ),
        });
    }
    Ok(Output {
        credential: Some(credential.into()),
        signature: Some(signature),
        headers: Some(vec![("APEX-PASSPHRASE".into(), passphrase.into())]),
    })
}
