//! Device-owned GRVT wallet graph; only account-bound order statements are signed.
use crate::hosted_wallet::Account;
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Operation, PayloadPart, Provider, SignError, SignRequest,
};

pub(super) fn sign(
    ids: &Identities,
    account: &Account,
    provider: &Provider,
    text: &str,
    format: &str,
    now: f64,
) -> Result<String, SignError> {
    let identity = ids.primary.as_ref().ok_or(SignError::Unavailable)?;
    let signing = identity.signing_ref()?.clone();
    let mut operations = vec![Operation::PublicAddress {
        key: signing.clone(),
    }];
    match format {
        "grvt-address" => {
            let work = SignRequest {
                receipt: Default::default(),
                context: Default::default(),
                operations: vec![Operation::Present {
                    key: identity.credential.clone(),
                }],
            };
            let output = provider.sign_ready(&work).ok_or(SignError::Unavailable)??;
            work.validate_result(&output)?;
            if !output.headers.is_empty() || output.values[0] != text.as_bytes() {
                return Err(SignError::InvalidRequest);
            }
        }
        "grvt-order" => {
            let subaccount = account
                .grvt_trading_account_id
                .as_deref()
                .ok_or(SignError::Unavailable)?;
            let digest = safe_cex_venue_grvt::provider::hosted_digest(
                text,
                subaccount,
                account.testnet,
                now,
            )?;
            operations.push(Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: signing,
                payload: vec![PayloadPart::Bytes {
                    bytes: digest.to_vec(),
                }],
            });
        }
        _ => return Err(SignError::Unsupported),
    }
    let work = SignRequest {
        receipt: Default::default(),
        context: Context {
            venue: "grvt".into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
        operations,
    };
    work.validate()?;
    let result = provider.sign_ready(&work).ok_or(SignError::Unavailable)??;
    work.validate_result(&result)?;
    if !result.headers.is_empty()
        || result.values[0].len() != 20
        || (format == "grvt-order" && result.values[1].len() != 65)
    {
        return Err(SignError::InvalidResult);
    }
    Ok(format!(
        "0x{}",
        safe_cex_crypto::hex(&result.values.concat())
    ))
}
