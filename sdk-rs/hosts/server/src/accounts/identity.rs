use crate::envelope::InitPayload;
use base64::Engine as _;
use safe_cex_crypto::admission::{Family, PublicKey};

const INVALID: &str = "Invalid account identity";

pub(super) fn hyperliquid_projection(
    user: &str,
    init: &InitPayload,
) -> Result<safe_cex_crypto::admission::identity::Projection, String> {
    let signer = init
        .hyperliquid_signer_address
        .as_deref()
        .ok_or("Missing Hyperliquid signer address")?;
    let account_scope = match init.vault.as_deref().filter(|address| !address.is_empty()) {
        Some(address) => serde_json::json!({"kind":"vault", "address":address}),
        None => serde_json::json!({"kind":"main"}),
    };
    let facts = serde_json::json!({
        "venue":init.exchange_name,
        "network":if init.testnet == Some(true) { "testnet" } else { "mainnet" },
        "credential":{"kind":"agent", "signer":signer, "accountScope":account_scope}
    });
    safe_cex_crypto::admission::identity::Projection::from_json(user, &facts.to_string())
        .map_err(|_| "Invalid Hyperliquid identity".into())
}

/// Freeze the same public signer and trading account used by GRVT construction.
/// A login credential or proof reply cannot substitute for this separate signer.
/// Venue authorization remains the ordinary engine startup's responsibility.
pub(super) fn grvt_projection(
    user: &str,
    init: &InitPayload,
) -> Result<safe_cex_crypto::admission::identity::Projection, String> {
    let invalid = || "Invalid account identity".to_owned();
    if init.exchange_name != "grvt" {
        return Err(invalid());
    }
    opaque(init)?;
    let signer = safe_cex_crypto::ethereum::get_address(
        init.grvt_signer_address.as_deref().ok_or_else(invalid)?,
    )
    .map_err(|_| invalid())?;
    let account = init
        .grvt_trading_account_id
        .as_deref()
        .ok_or_else(invalid)?;
    let facts = serde_json::json!({
        "venue": "grvt",
        "network": if init.testnet == Some(true) { "testnet" } else { "mainnet" },
        "credential": { "kind": "grvt", "tradingAccount": account, "signer": signer }
    });
    safe_cex_crypto::admission::identity::Projection::from_json(user, &facts.to_string())
        .map_err(|_| invalid())
}

pub(super) fn backpack_projection(
    user: &str,
    init: &InitPayload,
) -> Result<safe_cex_crypto::admission::identity::Projection, String> {
    use base64::{Engine, engine::general_purpose::STANDARD};
    let invalid = || "Invalid account identity".to_owned();
    if init.exchange_name != "backpack" {
        return Err(invalid());
    }
    let key = opaque(init)?;
    if key.len() != 44 {
        return Err(invalid());
    }
    let public = STANDARD.decode(&key).map_err(|_| invalid())?;
    if public.len() != 32 || STANDARD.encode(&public) != key {
        return Err(invalid());
    }
    let facts = serde_json::json!({
        "venue": "backpack",
        "network": if init.testnet == Some(true) { "testnet" } else { "mainnet" },
        "credential": { "kind": "ed25519", "publicKey": format!("0x{}", safe_cex_crypto::hex(&public)) }
    });
    safe_cex_crypto::admission::identity::Projection::from_json(user, &facts.to_string())
        .map_err(|_| invalid())
}

/// Lighter keeps its label identity: its registered-key admission owns that read.
pub(crate) fn enabled(init: &InitPayload) -> bool {
    crate::delegated::enabled(&init.exchange_name)
        && !matches!(init.exchange_name.as_str(), "lighter" | "lighter-robinhood")
        && init.public_only != Some(true)
        && !init.account_name.starts_with("__pub_")
}

fn opaque(init: &InitPayload) -> Result<String, String> {
    let trim = safe_cex_venues::signer_setup_policy(&init.exchange_name)
        .primary
        .and_then(|primary| primary.trim_credential)
        == Some(true);
    let key = if trim { init.key.trim() } else { &init.key };
    if key.is_empty() || key.len() > 1_048_576 || key.contains(['\r', '\n', '\0']) {
        return Err(INVALID.into());
    }
    Ok(key.into())
}

fn hex(text: &str, width: usize, padded: bool) -> Result<Vec<u8>, String> {
    let body = text
        .strip_prefix("0x")
        .or_else(|| text.strip_prefix("0X"))
        .ok_or(INVALID)?;
    if body.is_empty()
        || body.len() > width * 2
        || (!padded && body.len() != width * 2)
        || !body.bytes().all(|b| b.is_ascii_hexdigit())
    {
        return Err(INVALID.into());
    }
    let canonical = format!("{body:0>length$}", length = width * 2);
    (0..canonical.len())
        .step_by(2)
        .map(|i| u8::from_str_radix(&canonical[i..i + 2], 16).map_err(|_| INVALID.into()))
        .collect()
}

fn address(text: Option<&str>) -> Result<String, String> {
    let public = PublicKey::new(Family::Ethereum, &hex(text.ok_or(INVALID)?, 20, false)?)
        .map_err(|_| INVALID)?;
    Ok(format!("0x{}", safe_cex_crypto::hex(public.as_bytes())))
}

fn stark_address(text: Option<&str>) -> Result<String, String> {
    let bytes = hex(text.ok_or(INVALID)?, 32, true)?;
    let mut prime = [0u8; 32];
    prime[0] = 8;
    prime[7] = 17;
    prime[31] = 1;
    if bytes.iter().all(|b| *b == 0) || bytes.as_slice() >= prime.as_slice() {
        return Err(INVALID.into());
    }
    Ok(format!("0x{}", safe_cex_crypto::hex(&bytes)))
}

fn decimal(text: Option<&str>, max: u64) -> Result<u64, String> {
    let text = text.ok_or(INVALID)?.trim();
    if text.is_empty() || text.len() > 20 || !text.bytes().all(|b| b.is_ascii_digit()) {
        return Err(INVALID.into());
    }
    text.parse::<u64>()
        .ok()
        .filter(|n| *n <= max)
        .ok_or_else(|| INVALID.into())
}

pub(super) fn key_id(init: &InitPayload) -> Result<String, String> {
    let tuple = |fields: &[String]| serde_json::to_string(fields).expect("identity strings");
    match init.exchange_name.as_str() {
        "hyperliquid" | "hyperliquidspot" => Ok(tuple(&[
            address(Some(&init.key))?,
            address(init.hyperliquid_signer_address.as_deref())?,
            init.vault
                .as_deref()
                .filter(|v| !v.is_empty())
                .map(|v| address(Some(v)))
                .transpose()?
                .unwrap_or_default(),
        ])),
        "grvt" => Ok(tuple(&[
            decimal(init.grvt_trading_account_id.as_deref(), u64::MAX)?.to_string(),
            address(init.grvt_signer_address.as_deref())?,
        ])),
        "paradex" => {
            let public = PublicKey::new(
                Family::Stark,
                &hex(init.paradex_public_key.as_deref().ok_or(INVALID)?, 32, true)?,
            )
            .map_err(|_| INVALID)?;
            Ok(tuple(&[
                stark_address(init.paradex_address.as_deref())?,
                format!("0x{}", safe_cex_crypto::hex(public.as_bytes())),
            ]))
        }
        "backpack" => {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(opaque(init)?)
                .map_err(|_| INVALID)?;
            let public = PublicKey::new(Family::Ed25519, &bytes).map_err(|_| INVALID)?;
            Ok(base64::engine::general_purpose::STANDARD.encode(public.as_bytes()))
        }
        _ => opaque(init),
    }
}

fn family(venue: &str) -> &str {
    match venue {
        "bybitinverse" | "bybitspot" | "bybitdemo" | "hyrotrader" => "bybit",
        "binancepm" | "binanceinverse" | "binancespot" | "binanceoptions" => "binance",
        "okxeu" | "okxdemo" | "okxeudemo" => "okx",
        "bitgetinverse" | "bitgetusdc" => "bitget",
        "kucoinspot" => "kucoin",
        "hyperliquidspot" => "hyperliquid",
        "bingxspot" => "bingx",
        "coinbasespot" => "coinbase",
        _ => venue,
    }
}

fn scope(init: &InitPayload) -> String {
    let venue = init.exchange_name.as_str();
    let family = family(venue);
    let testnet = init.testnet == Some(true);
    let network = match venue {
        "okx" | "okxeu" | "okxdemo" | "okxeudemo" if testnet || venue.ends_with("demo") => "demo",
        "bybitdemo" if !testnet => "demo",
        "binancespot" if testnet => "spot-testnet",
        _ if testnet => "testnet",
        _ => "mainnet",
    };
    let site = match venue {
        "bybit" | "bybitinverse" | "bybitspot" => init.bybit_site_id.as_deref().unwrap_or_default(),
        "okxeu" | "okxeudemo" => "eu",
        "hyrotrader" => "hyrotrader",
        _ => "",
    };
    serde_json::to_string(&[family, network, site]).expect("identity scope strings")
}

pub(crate) fn digest(user: &str, init: &InitPayload) -> Result<Option<String>, String> {
    if !enabled(init) {
        return Ok(None);
    }
    if user.is_empty() {
        return Err("Authenticated user required".into());
    }
    let (scope, key) = (scope(init), key_id(init)?);
    let mut input = b"tealstreet/identity/v1".to_vec();
    for component in [user, scope.as_str(), key.as_str()] {
        input.extend_from_slice(&(component.len() as u64).to_be_bytes());
        input.extend_from_slice(component.as_bytes());
    }
    Ok(Some(safe_cex_crypto::hex(&safe_cex_crypto::sha256(&input))))
}

/// Sibling products share a credential namespace, but need separate actors and
/// algo owners. Environment aliases select the same product, with network/site
/// already bound by the credential scope. Preserve each base product's identity.
pub(super) fn actor_digest(user: &str, init: &InitPayload) -> Result<Option<String>, String> {
    let Some(credential) = digest(user, init)? else {
        return Ok(None);
    };
    let product = match init.exchange_name.as_str() {
        "okxdemo" => "okx",
        "okxeudemo" => "okxeu",
        venue => venue,
    };
    if product == family(product) {
        return Ok(Some(credential));
    }
    let mut input = b"tealstreet/actor/v1".to_vec();
    for component in [credential.as_str(), product] {
        input.extend_from_slice(&(component.len() as u64).to_be_bytes());
        input.extend_from_slice(component.as_bytes());
    }
    Ok(Some(safe_cex_crypto::hex(&safe_cex_crypto::sha256(&input))))
}

pub(super) fn registry_name(user: &str, init: &InitPayload) -> Result<String, String> {
    Ok(actor_digest(user, init)?.unwrap_or_else(|| init.account_name.clone()))
}

// A slot queue is intentionally stricter than a registered-key identity: rotation
// and alternate client labels cannot run overlapping proofs for the same slot.
pub(super) fn lighter_queue(user: &str, init: &InitPayload) -> Result<[u8; 32], String> {
    let (account, index) = lighter_indices(init)?;
    if user.is_empty() {
        return Err("Invalid account identity".into());
    }
    let mut bytes = b"tealstreet/proof-queue/lighter-slot/v1".to_vec();
    for field in [
        user,
        init.exchange_name.as_str(),
        if init.testnet == Some(true) {
            "testnet"
        } else {
            "mainnet"
        },
        &account.to_string(),
        &index.to_string(),
    ] {
        bytes.extend_from_slice(&(field.len() as u64).to_be_bytes());
        bytes.extend_from_slice(field.as_bytes());
    }
    Ok(safe_cex_crypto::sha256(&bytes))
}

pub(super) fn lighter_indices(init: &InitPayload) -> Result<(u64, u8), String> {
    let invalid = || "Invalid account identity".to_owned();
    if !matches!(init.exchange_name.as_str(), "lighter" | "lighter-robinhood") {
        return Err(invalid());
    }
    let account = init.application_id.as_deref().ok_or_else(invalid)?;
    let index = init.api_key_index.as_deref().ok_or_else(invalid)?;
    if account.is_empty()
        || account.len() > 20
        || !account.bytes().all(|b| b.is_ascii_digit())
        || index.is_empty()
        || index.len() > 3
        || !index.bytes().all(|b| b.is_ascii_digit())
    {
        return Err(invalid());
    }
    let account = account.parse().map_err(|_| invalid())?;
    let index = index.parse().map_err(|_| invalid())?;
    let network = safe_cex_venues::lighter_registration::Network::from_options(
        init.exchange_name == "lighter-robinhood",
        init.testnet == Some(true),
    );
    safe_cex_venues::lighter_registration::request(network, account, index)
        .map_err(|_| invalid())?;
    Ok((account, index))
}
