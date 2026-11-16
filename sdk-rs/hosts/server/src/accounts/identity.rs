use crate::envelope::InitPayload;

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
