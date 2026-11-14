//! Device-owned Aster signing: slot-bound HMAC or account-bound wallet statements.
use crate::hosted_wallet::Account;
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Operation, PayloadPart, Provider, SignError, SignRequest,
};
use serde::{Deserialize, Serialize};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
enum Slot {
    Primary,
    Secondary,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Descriptor {
    slot: Slot,
    payload: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Address {
    slot: Slot,
}

pub(super) fn sign(
    ids: &Identities,
    account: &Account,
    provider: &Provider,
    text: &str,
    format: &str,
    now: f64,
) -> Result<String, SignError> {
    if text.len() > 1_048_576 {
        return Err(SignError::InvalidRequest);
    }
    let address_only = format == "asterdex-address";
    let (slot, payload) = if address_only {
        let input: Address = serde_json::from_str(text).map_err(|_| SignError::InvalidRequest)?;
        (input.slot, String::new())
    } else {
        let input: Descriptor =
            serde_json::from_str(text).map_err(|_| SignError::InvalidRequest)?;
        if input.payload.is_empty() {
            return Err(SignError::InvalidRequest);
        }
        (input.slot, input.payload)
    };
    let identity = match slot {
        Slot::Primary => &ids.primary,
        Slot::Secondary => &ids.secondary,
    }
    .as_ref()
    .ok_or(SignError::Unavailable)?;
    identity.validate().map_err(|_| SignError::InvalidRequest)?;
    let signing = identity.signing_ref()?.clone();
    let execute = |operations| {
        let work = SignRequest {
            receipt: Default::default(),
            context: Context {
                venue: "asterdex".into(),
                method: "HOSTED".into(),
                ..Default::default()
            },
            operations,
        };
        work.validate()?;
        let mut output = provider.sign_ready(&work).ok_or(SignError::Unavailable)??;
        work.validate_result(&output)?;
        if !output.headers.is_empty() {
            return Err(SignError::InvalidResult);
        }
        Ok(std::mem::take(&mut output.values))
    };
    let (algorithm, bytes, width) = match format {
        "asterdex-address" | "asterdex-wallet" if account.asterdex_v3 => {
            let address = execute(vec![Operation::PublicAddress {
                key: signing.clone(),
            }])?;
            let [address] = address.as_slice() else {
                return Err(SignError::InvalidResult);
            };
            if address.len() != 20 {
                return Err(SignError::InvalidResult);
            }
            let own = format!("0x{}", safe_cex_crypto::hex(address));
            if address_only {
                return Ok(own);
            }
            let user = account
                .asterdex_user_address
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .or_else(|| {
                    account
                        .application_id
                        .as_deref()
                        .map(str::trim)
                        .filter(|value| !value.is_empty())
                })
                .ok_or(SignError::Unavailable)?;
            let digest = safe_cex_venue_asterdex::sign::hosted_digest(
                &payload,
                user,
                &own,
                account.testnet,
                now,
            )?;
            (Algorithm::Secp256k1Digest, digest.to_vec(), 65)
        }
        "asterdex-hmac" if !account.asterdex_v3 => {
            (Algorithm::HmacSha256, payload.into_bytes(), 32)
        }
        _ => return Err(SignError::Unsupported),
    };
    let values = execute(vec![
        Operation::Present {
            key: identity.credential.clone(),
        },
        Operation::Sign {
            algorithm,
            key: signing,
            payload: vec![PayloadPart::Bytes { bytes }],
        },
    ])?;
    if values.len() != 2 || values[1].len() != width {
        return Err(SignError::InvalidResult);
    }
    let credential = std::str::from_utf8(&values[0]).map_err(|_| SignError::InvalidResult)?;
    if credential.is_empty() || credential.contains(['\0', '\r', '\n']) {
        return Err(SignError::InvalidResult);
    }
    #[derive(Serialize)]
    struct Batch<'a> {
        credential: &'a str,
        signature: String,
    }
    serde_json::to_string(&Batch {
        credential,
        signature: safe_cex_crypto::hex(&values[1]),
    })
    .map_err(|_| SignError::InvalidResult)
}
