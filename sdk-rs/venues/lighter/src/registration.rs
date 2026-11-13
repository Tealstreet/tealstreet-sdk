//! Public registration reads using the same network origins as the venue.
//! Wire reference: elliottech/lighter-python docs/AccountApi.md#apikeys.
use safe_cex_engine::{HttpRequest, Method, RequestId};
use serde::Deserialize;

pub const MAX_RESPONSE_BYTES: usize = 16_384;
pub const TIMEOUT_MS: u32 = 20_000;

#[derive(Clone, Copy)]
pub enum Network {
    Mainnet,
    Testnet,
    Robinhood,
    RobinhoodTestnet,
}
impl Network {
    pub fn from_options(robinhood: bool, testnet: bool) -> Self {
        match (robinhood, testnet) {
            (false, false) => Self::Mainnet,
            (false, true) => Self::Testnet,
            (true, false) => Self::Robinhood,
            (true, true) => Self::RobinhoodTestnet,
        }
    }
    pub fn origin(self) -> &'static str {
        match self {
            Self::Mainnet => "https://mainnet.zklighter.elliot.ai",
            Self::Testnet => "https://testnet-api.lighter.xyz",
            Self::Robinhood => "https://api.rh.lighter.xyz",
            Self::RobinhoodTestnet => "https://api.rh-testnet.lighter.xyz",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Refused;

fn valid_indices(account: u64, index: u8) -> bool {
    // Reuse the venue's account/index bounds; no transaction is constructed.
    super::sign::Account::new(0, account, index).is_ok()
}
pub fn request(network: Network, account: u64, index: u8) -> Result<HttpRequest, Refused> {
    if !valid_indices(account, index) {
        return Err(Refused);
    }
    Ok(HttpRequest {
        id: RequestId(1),
        method: Method::Get,
        route: None,
        timeout_ms: Some(TIMEOUT_MS),
        url: format!(
            "{}/api/v1/apikeys?account_index={account}&api_key_index={index}",
            network.origin()
        ),
        headers: vec![],
        signer_headers: vec![],
        body: None,
    })
}

#[derive(Deserialize)]
struct Response {
    code: u32,
    api_keys: Vec<Row>,
}
#[derive(Deserialize)]
struct Row {
    account_index: u64,
    api_key_index: u8,
    public_key: String,
}

pub fn decode(body: &[u8], account: u64, index: u8) -> Result<[u8; 40], Refused> {
    if body.len() > MAX_RESPONSE_BYTES || !valid_indices(account, index) {
        return Err(Refused);
    }
    let response: Response = serde_json::from_slice(body).map_err(|_| Refused)?;
    if response.code != 200 || response.api_keys.len() != 1 {
        return Err(Refused);
    }
    let row = &response.api_keys[0];
    if row.account_index != account || row.api_key_index != index {
        return Err(Refused);
    }
    let hex = row.public_key.strip_prefix("0x").unwrap_or(&row.public_key);
    if hex.len() != 80 || !hex.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(Refused);
    }
    let mut public = [0; 40];
    for (out, pair) in public.iter_mut().zip(hex.as_bytes().as_chunks::<2>().0) {
        *out = u8::from_str_radix(std::str::from_utf8(pair).map_err(|_| Refused)?, 16)
            .map_err(|_| Refused)?;
    }
    if !safe_cex_crypto::lighter::is_public_key(&public) {
        return Err(Refused);
    }
    Ok(public)
}
