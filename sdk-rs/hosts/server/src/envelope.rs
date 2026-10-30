//! Today's `wss /account` JSON envelope (NX `services/exchange-server/src/server.ts`).
use serde::{Deserialize, Serialize};
use serde_json::{Value, json};

/// Non-secret account fields the browser sends; there is never an API secret.
#[derive(Clone, Debug, Default, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct InitPayload {
    pub account_name: String,
    pub exchange_name: String,
    #[serde(default)]
    pub key: String,
    pub passphrase: Option<String>,
    pub testnet: Option<bool>,
    pub application_id: Option<String>,
    pub api_key_index: Option<String>,
    pub paradex_address: Option<String>,
    pub subkey_address: Option<String>,
    pub subaccount: Option<String>,
    pub orderly_exchange_id: Option<String>,
    pub grvt_trading_account_id: Option<String>,
    pub tealstreet_proxy: Option<bool>,
    pub tealstreet_tunnel: Option<bool>,
    pub tealstreet_ws_route: Option<bool>,
    pub proxy_region: Option<String>,
    pub public_only: Option<bool>,
    pub hyperliquid_builder: Option<HyperliquidBuilder>,
    /// The Hyperliquid vault the account trades; without it orders go to the main wallet.
    pub vault: Option<String>,
    /// Bybit's regional site (`BRA_BTL` selects the Brazil host and header).
    pub bybit_site_id: Option<String>,
    pub enable_spot_markets: Option<bool>,
    pub enable_inverse_markets: Option<bool>,
    pub enable_option_markets: Option<bool>,
    /// Diagnostics only; excluded from the init fingerprint.
    #[serde(skip_serializing)]
    pub verbose: Option<bool>,
}

#[derive(Clone, Debug, Deserialize, Serialize, PartialEq)]
#[serde(rename_all = "camelCase")]
pub struct HyperliquidBuilder {
    pub address: String,
    pub fee_tenths_bps: f64,
}

impl InitPayload {
    /// Identifies what an account was built from; an identical resumed init keeps it.
    pub fn fingerprint(&self) -> String {
        serde_json::to_string(self).expect("init payload serializes")
    }
}

#[derive(Debug, Deserialize)]
pub struct AccountMsg {
    pub op: String,
    #[serde(default)]
    pub data: Value,
    #[serde(rename = "accountName")]
    pub account_name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t", content = "d", rename_all = "kebab-case")]
pub enum Inbound {
    Init(Box<InitPayload>),
    Msg(AccountMsg),
    SignRes(Value),
    Dispose {
        #[serde(rename = "accountName")]
        account_name: String,
    },
    EchoRes(Value),
    Ping(Option<PingData>),
    Perf(Value),
    Verbose(Value),
}

#[derive(Debug, Default, Deserialize)]
pub struct PingData {
    #[serde(default)]
    pub accounts: Vec<String>,
}

pub fn msg(account_name: &str, op: &str, data: Value) -> String {
    json!({"t": "msg", "d": {"op": op, "accountName": account_name, "data": data}}).to_string()
}

pub fn frame(account_name: &str, seq: u64, snapshot: bool, scxf: &[u8]) -> String {
    use base64::Engine as _;
    json!({"t": "msg", "d": {
        "op": "frame",
        "accountName": account_name,
        "seq": seq,
        "snapshot": snapshot,
        "data": base64::engine::general_purpose::STANDARD.encode(scxf),
    }})
    .to_string()
}

pub fn error(account_name: Option<&str>, message: &str) -> String {
    json!({"t": "msg", "d": {"op": "error", "accountName": account_name, "data": [message]}})
        .to_string()
}

pub fn session(resume_id: &str, resumed: bool) -> String {
    json!({"t": "session", "d": {"resumeId": resume_id, "resumed": resumed}}).to_string()
}

pub fn pong(accounts: &[String]) -> String {
    json!({"t": "pong", "d": {"accounts": accounts}}).to_string()
}
