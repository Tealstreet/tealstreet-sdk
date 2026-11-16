use crate::envelope::InitPayload;

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
    key_id(init)?;
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
    let key = key_id(init)?;
    if key.len() != 44 {
        return Err(invalid());
    }
    let public = STANDARD.decode(key).map_err(|_| invalid())?;
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

pub(crate) fn enabled(init: &InitPayload) -> bool {
    init.exchange_name == "bybit"
        && crate::delegated::enabled(&init.exchange_name)
        && init.public_only != Some(true)
        && !init.account_name.starts_with("__pub_")
}

pub(super) fn key_id(init: &InitPayload) -> Result<&str, String> {
    let trim = safe_cex_venues::signer_setup_policy(&init.exchange_name)
        .primary
        .and_then(|primary| primary.trim_credential)
        == Some(true);
    let key = if trim { init.key.trim() } else { &init.key };
    if key.is_empty() || key.len() > 1_048_576 || key.contains(['\r', '\n', '\0']) {
        return Err("Invalid account identity".into());
    }
    Ok(key)
}

pub(crate) fn digest(user: &str, init: &InitPayload) -> Result<Option<String>, String> {
    if !enabled(init) {
        return Ok(None);
    }
    if user.is_empty() {
        return Err("Authenticated user required".into());
    }
    let network = if init.testnet == Some(true) {
        "testnet"
    } else {
        "mainnet"
    };
    let scope = serde_json::to_string(&[
        init.exchange_name.as_str(),
        network,
        init.bybit_site_id.as_deref().unwrap_or_default(),
    ])
    .expect("identity scope strings");
    let mut input = b"tealstreet/identity/v1".to_vec();
    for component in [user, scope.as_str(), key_id(init)?] {
        input.extend_from_slice(&(component.len() as u64).to_be_bytes());
        input.extend_from_slice(component.as_bytes());
    }
    Ok(Some(
        safe_cex_crypto::sha256(&input)
            .iter()
            .map(|byte| format!("{byte:02x}"))
            .collect(),
    ))
}

pub(super) fn registry_name(user: &str, init: &InitPayload) -> Result<String, String> {
    Ok(digest(user, init)?.unwrap_or_else(|| init.account_name.clone()))
}
