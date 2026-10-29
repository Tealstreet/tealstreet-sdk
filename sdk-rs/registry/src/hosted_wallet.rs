//! Device-side wallet signing for keyless servers. The device never signs a
//! digest it did not derive itself from an allowlisted, account-bound statement.
//! Synchronous over the local provider, without the HMAC capture codec.
use safe_cex_engine::signing::{
    Algorithm, Context, Identities, Operation, PayloadPart, Provider, SignError, SignRequest,
};

/// The signed-in account a device signs for, captured at construction.
#[derive(Clone, Debug, Default)]
pub struct Account {
    pub key: Option<String>,
    pub vault: Option<String>,
    pub testnet: bool,
}

pub fn admitted(venue: &str) -> bool {
    cfg!(feature = "hyperliquid") && matches!(venue, "hyperliquid" | "hyperliquidspot")
}

/// Answer one remote ws-auth wallet request with `0x` hex: `hyperliquid-address`
/// returns the agent address, `hyperliquid-l1` returns `r || s || v` (v 27/28).
/// `now_ms` is the device's protocol clock, for families whose statements expire.
pub fn sign(
    venue: &str,
    identities: &Identities,
    account: &Account,
    provider: &Provider,
    to_sign: &str,
    output_format: &str,
    now_ms: f64,
) -> Result<String, SignError> {
    let _ = now_ms;
    if !admitted(venue) {
        return Err(SignError::Unsupported);
    }
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    let signing = identity.signing_ref()?.clone();
    let mut operations = vec![Operation::PublicAddress {
        key: signing.clone(),
    }];
    match output_format {
        "hyperliquid-address" => {
            let own = account.key.as_deref().ok_or(SignError::Unavailable)?;
            if !to_sign.eq_ignore_ascii_case(own) {
                return Err(SignError::InvalidRequest);
            }
        }
        #[cfg(feature = "hyperliquid")]
        "hyperliquid-l1" => {
            use safe_cex_venue_hyperliquid::sign::statement;
            let digest = statement::derive(
                to_sign,
                &statement::Account {
                    mainnet: !account.testnet,
                    vault: account.vault.as_deref(),
                },
            )?;
            operations.push(Operation::Sign {
                algorithm: Algorithm::Secp256k1Digest,
                key: signing,
                payload: vec![PayloadPart::Bytes {
                    bytes: digest.to_vec(),
                }],
            });
        }
        _ => return Err(SignError::InvalidRequest),
    }
    let work = SignRequest {
        receipt: Default::default(),
        context: Context {
            venue: venue.into(),
            method: "HOSTED".into(),
            ..Default::default()
        },
        operations,
    };
    work.validate()?;
    let result = provider.sign_ready(&work).ok_or(SignError::Unavailable)??;
    work.validate_result(&result)?;
    if !result.headers.is_empty() {
        return Err(SignError::InvalidResult);
    }
    let value = match result.values.as_slice() {
        [address] if address.len() == 20 && work.operations.len() == 1 => address.clone(),
        [address, signature] if address.len() == 20 && signature.len() == 65 => {
            let mut signature = signature.clone();
            match &mut signature[64] {
                v @ (0 | 1) => *v += 27,
                27 | 28 => {}
                _ => return Err(SignError::InvalidResult),
            }
            signature
        }
        _ => return Err(SignError::InvalidResult),
    };
    Ok(format!("0x{}", safe_cex_crypto::hex(&value)))
}
