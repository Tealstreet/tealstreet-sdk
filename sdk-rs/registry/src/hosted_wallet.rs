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
    /// Lighter: the account index, when configured as the Application ID.
    pub application_id: Option<String>,
    pub api_key_index: Option<String>,
}

pub fn admitted(venue: &str) -> bool {
    (cfg!(feature = "hyperliquid") && matches!(venue, "hyperliquid" | "hyperliquidspot"))
        || lighter(venue)
}

fn lighter(venue: &str) -> bool {
    cfg!(feature = "lighter") && matches!(venue, "lighter" | "lighter-robinhood")
}

/// Lighter signs only words it re-derives from an allowlisted statement for this
/// deployment's chain and this account's API key, and only while the statement's
/// expiry is ahead of the device clock by no more than the venue's lifetime.
#[cfg(feature = "lighter")]
fn lighter_sign(
    venue: &str,
    identities: &Identities,
    account: &Account,
    provider: &Provider,
    to_sign: &str,
    now_ms: f64,
) -> Result<String, SignError> {
    use safe_cex_venue_lighter::sign::{chain_id, statement};
    // Engine and device clocks may differ by this much at most.
    const SKEW_MS: f64 = 30_000.;
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    let decimal = |value: Option<&str>| value.and_then(|text| text.trim().parse::<u64>().ok());
    let api_key_index = decimal(account.api_key_index.as_deref())
        .and_then(|index| u8::try_from(index).ok())
        .ok_or(SignError::Unavailable)?;
    let owner = statement::Owner {
        chain_id: chain_id(venue == "lighter-robinhood", account.testnet),
        account_index: decimal(account.application_id.as_deref()),
        api_key_index,
    };
    let derived = statement::derive(to_sign, &owner).map_err(|_| SignError::InvalidRequest)?;
    let expiry = derived.expiry_ms as f64;
    if !now_ms.is_finite()
        || expiry <= now_ms
        || expiry > now_ms + statement::MAX_LIFETIME_MS as f64 + SKEW_MS
    {
        return Err(SignError::InvalidRequest);
    }
    let signing = identity.signing_ref()?.clone();
    // The hedged nonce is keyed by the scalar and the message, so fixed entropy
    // degrades to deterministic signing, never to a reused nonce.
    let operations = derived
        .preimages
        .into_iter()
        .map(|preimage| {
            let mut bytes = vec![0; 32];
            bytes.extend_from_slice(&preimage);
            Operation::Sign {
                algorithm: Algorithm::SchnorrDigest,
                key: signing.clone(),
                payload: vec![PayloadPart::Bytes { bytes }],
            }
        })
        .collect::<Vec<_>>();
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
    if !result.headers.is_empty() || result.values.iter().any(|value| value.len() != 80) {
        return Err(SignError::InvalidResult);
    }
    Ok(format!(
        "0x{}",
        safe_cex_crypto::hex(&result.values.concat())
    ))
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
    if !admitted(venue) {
        return Err(SignError::Unsupported);
    }
    // Lighter signs nothing but its own statements, and no other family signs them.
    if lighter(venue) != (output_format == "lighter-tx") {
        return Err(SignError::InvalidRequest);
    }
    #[cfg(feature = "lighter")]
    if lighter(venue) {
        return lighter_sign(venue, identities, account, provider, to_sign, now_ms);
    }
    let _ = now_ms;
    let identity = identities.primary.as_ref().ok_or(SignError::Unavailable)?;
    let signing = identity.signing_ref()?.clone();
    // A Lighter-only build has no Hyperliquid signature arm to push.
    #[cfg_attr(not(feature = "hyperliquid"), allow(unused_mut))]
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
