use crate::envelope::InitPayload;
use base64::Engine as _;
use safe_cex_crypto::admission::{Family, PublicKey};

const INVALID: &str = "Invalid account identity";

pub(crate) fn enabled(init: &InitPayload) -> bool {
    safe_cex_venues::remote_signing_policy(&init.exchange_name).enabled
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

pub(super) fn decimal(text: Option<&str>, max: u64) -> Result<u64, String> {
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
            address(init.signer_address.as_deref())?,
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
        "lighter" | "lighter-robinhood" => {
            let account = decimal(init.application_id.as_deref(), i64::MAX as u64)?;
            let index = decimal(init.api_key_index.as_deref(), 254)? as u8;
            let registered = init.lighter_registered_key.as_ref().ok_or(INVALID)?;
            if registered.account != account
                || registered.index != index
                || registered.scope != scope(init)
            {
                return Err(INVALID.into());
            }
            Ok(tuple(&[
                account.to_string(),
                index.to_string(),
                safe_cex_crypto::hex(&registered.public),
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

pub(super) fn scope(init: &InitPayload) -> String {
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
    if let Some(registered) = &init.lighter_registered_key {
        if registered.user != user {
            return Err("Account identity belongs to another owner".into());
        }
        registered.check()?;
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

#[derive(Clone)]
pub struct RegisteredLighterKey {
    user: String,
    scope: String,
    allowed: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    account: u64,
    index: u8,
    public: Vec<u8>,
}

impl std::fmt::Debug for RegisteredLighterKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RegisteredLighterKey")
    }
}
impl PartialEq for RegisteredLighterKey {
    fn eq(&self, other: &Self) -> bool {
        self.user == other.user
            && self.scope == other.scope
            && self.account == other.account
            && self.index == other.index
            && self.public == other.public
    }
}
impl RegisteredLighterKey {
    pub(super) fn check(&self) -> Result<(), String> {
        if (self.allowed)() {
            Ok(())
        } else {
            Err("Account identity resolution expired".into())
        }
    }
    pub(super) fn new(
        user: String,
        scope: String,
        account: u64,
        index: u8,
        public: &PublicKey,
        allowed: std::sync::Arc<dyn Fn() -> bool + Send + Sync>,
    ) -> Result<Self, String> {
        if public.family() != Family::Schnorr {
            return Err(INVALID.into());
        }
        Ok(Self {
            user,
            scope,
            allowed,
            account,
            index,
            public: public.as_bytes().to_vec(),
        })
    }
}
