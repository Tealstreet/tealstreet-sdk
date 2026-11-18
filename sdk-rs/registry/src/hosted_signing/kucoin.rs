//! Device-owned KuCoin batch. No passphrase or broker key crosses the wire.
use super::{Output, OutputKind};
use safe_cex_engine::signing::{
    Algorithm, Context, ExtraKey, Identities, Operation, PayloadPart, SignError, SignReceipt,
    SignRequest, SignResult,
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
    broker_prefix: Option<String>,
}

pub(super) fn prepare(
    venue: &str,
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
    let passphrase = identity.passphrase.as_ref().ok_or(SignError::Unavailable)?;
    let sign = |key, payload| Operation::Sign {
        algorithm: Algorithm::HmacSha256,
        key,
        payload,
    };
    let mut operations = vec![
        Operation::Present {
            key: identity.credential.clone(),
        },
        sign(
            identity.signing_ref()?.clone(),
            vec![PayloadPart::Bytes {
                bytes: input.payload.into_bytes(),
            }],
        ),
        sign(
            identity.signing_ref()?.clone(),
            vec![PayloadPart::Credential {
                key: passphrase.clone(),
            }],
        ),
    ];
    let broker = input.broker_prefix.is_some();
    if let Some(prefix) = input.broker_prefix {
        if prefix.is_empty() {
            return Err(SignError::InvalidRequest);
        }
        operations.push(sign(
            identity
                .extra
                .get(&ExtraKey::KucoinBroker)
                .ok_or(SignError::Unavailable)?
                .clone(),
            vec![
                PayloadPart::Bytes {
                    bytes: prefix.into_bytes(),
                },
                PayloadPart::Credential {
                    key: identity.credential.clone(),
                },
            ],
        ));
    }
    let work = SignRequest {
        receipt,
        context: Context {
            venue: venue.into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
        operations,
    };
    work.validate()?;
    Ok((work, OutputKind::Kucoin { broker, ws }))
}

pub(super) fn finish(broker: bool, ws: bool, result: &SignResult) -> Result<Output, SignError> {
    if !result.headers.is_empty()
        || result.values.len() != if broker { 4 } else { 3 }
        || result.values[1..].iter().any(|mac| mac.len() != 32)
    {
        return Err(SignError::InvalidResult);
    }
    let credential =
        std::str::from_utf8(&result.values[0]).map_err(|_| SignError::InvalidResult)?;
    if credential.is_empty() || credential.contains(['\r', '\n', '\0']) {
        return Err(SignError::InvalidResult);
    }
    let macs: Vec<_> = result.values[1..]
        .iter()
        .map(|mac| safe_cex_crypto::encode(mac, safe_cex_crypto::Encoding::Base64))
        .collect();
    if ws {
        #[derive(Serialize)]
        struct Batch<'a> {
            credential: &'a str,
            macs: Vec<String>,
        }
        return Ok(Output {
            credential: None,
            headers: None,
            signature: Some(
                serde_json::to_string(&Batch { credential, macs })
                    .map_err(|_| SignError::InvalidResult)?,
            ),
        });
    }
    let mut headers = vec![("KC-API-PASSPHRASE".into(), macs[1].clone())];
    if broker {
        headers.push(("KC-API-PARTNER-SIGN".into(), macs[2].clone()));
    }
    Ok(Output {
        credential: Some(credential.into()),
        signature: Some(macs[0].clone()),
        headers: Some(headers),
    })
}
