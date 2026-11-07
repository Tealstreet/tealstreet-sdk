//! Exact Coinbase JWS bytes; the owned provider checks its key and algorithm.
use super::*;
use safe_cex_engine::signing::{Algorithm, PayloadPart};
use safe_cex_venue_coinbase::sign::JwtAlgorithm;

pub(super) fn prepare(
    ids: &Identities,
    input: String,
) -> Result<(SignRequest, OutputKind), SignError> {
    let (binding, nbf) = safe_cex_venue_coinbase::sign::inspect_input(&input)
        .map_err(|_| SignError::InvalidRequest)?;
    let identity = ids.primary.as_ref().ok_or(SignError::Unavailable)?;
    let expiry = nbf
        .checked_add(120)
        .and_then(|s| s.checked_mul(1000))
        .ok_or(SignError::InvalidRequest)?;
    let work = SignRequest {
        receipt: SignReceipt {
            stamp_ms: Some(nbf as f64 * 1000.),
            expiry_ms: Some(expiry as f64),
            ..Default::default()
        },
        context: Context {
            venue: "coinbase".into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
        operations: vec![
            Operation::Present {
                key: identity.credential.clone(),
            },
            Operation::Sign {
                algorithm: match binding.algorithm {
                    JwtAlgorithm::Es256 => Algorithm::Es256,
                    JwtAlgorithm::EdDsa => Algorithm::Ed25519,
                },
                key: identity.signing_ref()?.clone(),
                payload: vec![PayloadPart::Bytes {
                    bytes: input.into_bytes(),
                }],
            },
        ],
    };
    work.validate()?;
    Ok((work, OutputKind::Coinbase))
}
pub(super) fn finish(result: &SignResult) -> Result<Output, SignError> {
    if !result.headers.is_empty() || result.values.len() != 2 || result.values[1].len() != 64 {
        return Err(SignError::InvalidResult);
    }
    let credential =
        std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
    if credential.is_empty() || credential.contains(['\r', '\n', '\0']) {
        return Err(SignError::InvalidResult);
    }
    let signature = safe_cex_crypto::encode(&result.values[1], safe_cex_crypto::Encoding::Base64)
        .trim_end_matches('=')
        .replace('+', "-")
        .replace('/', "_");
    Ok(Output {
        credential: None,
        signature: Some(signature),
        headers: None,
    })
}
