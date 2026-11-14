//! Canonical account/options contracts; opaque host capabilities retain TS ownership.
use crate::{
    ExchangeName, PayloadSignerCapability, PublicDataSourceCapability, RemoteSignerCapability,
    TransportCapability, deserialize_present,
};
use serde::{Deserialize, Serialize};
use ts_rs::TS;

/// Host-only signer setup cannot be decoded from a JSON object.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum LocalSignerMetadataCapability {}
/// The shared public-feed port is a live host capability, never a JSON record.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RustPublicFeedCapability {}

/// Account-worker signing modes. Absence preserves legacy caller selection.
/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum SigningMode {
    #[serde(rename = "local")]
    Local,
    #[serde(rename = "cosigned")]
    Cosigned,
    #[serde(rename = "delegated")]
    Delegated,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum CoinbaseJwtAlgorithm {
    #[serde(rename = "es256")]
    Es256,
    #[serde(rename = "edDsa")]
    EdDsa,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeAccountConfigBybitSiteId {
    #[serde(rename = "BRA_BTL")]
    BraBtl,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeAccountConfigHealthStatus {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "auth_error")]
    AuthError,
    #[serde(rename = "auth_expired")]
    AuthExpired,
    #[serde(rename = "permissions_error")]
    PermissionsError,
    #[serde(rename = "startup_failed")]
    StartupFailed,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeAccountConfig {
    /// @ts-mutable
    #[serde(rename = "exchange")]
    pub exchange: ExchangeName,
    /// @ts-mutable
    #[serde(rename = "name")]
    pub name: String,
    /// @ts-mutable
    #[serde(rename = "testnet")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub testnet: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "apiKey")]
    pub api_key: String,
    /// @ts-mutable
    #[serde(rename = "apiSecret")]
    pub api_secret: String,
    /// @ts-mutable
    /// Account-worker mode; absent keeps existing signing selection.
    #[serde(rename = "signingMode")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub signing_mode: Option<SigningMode>,
    /// @ts-mutable
    #[serde(rename = "passphrase")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub passphrase: Option<String>,
    /// @ts-mutable
    #[serde(rename = "secondary_apiKey")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_api_key: Option<String>,
    /// @ts-mutable
    #[serde(rename = "secondary_apiSecret")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_api_secret: Option<String>,
    /// @ts-mutable
    #[serde(rename = "secondary_passphrase")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_passphrase: Option<String>,
    /// @ts-mutable
    #[serde(rename = "enableSpotMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_spot_markets: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "enableOptionMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_option_markets: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "enableInverseMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_inverse_markets: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "bybitSiteId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub bybit_site_id: Option<ExchangeAccountConfigBybitSiteId>,
    /// @ts-mutable
    #[serde(rename = "applicationId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub application_id: Option<String>,
    /// @ts-mutable
    #[serde(rename = "apiKeyIndex")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key_index: Option<String>,
    /// @ts-mutable
    #[serde(rename = "vault")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub vault: Option<String>,
    /// @ts-mutable
    #[serde(rename = "omniKeySeed")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub omni_key_seed: Option<String>,
    /// @ts-mutable
    #[serde(rename = "subaccount")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub subaccount: Option<String>,
    /// @ts-mutable
    #[serde(rename = "orderlyExchangeId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub orderly_exchange_id: Option<String>,
    /// @ts-mutable
    #[serde(rename = "paradexAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub paradex_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "subkeyAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub subkey_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "grvtSignerAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub grvt_signer_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "grvtTradingAccountId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub grvt_trading_account_id: Option<String>,
    /// Hosted CDP key algorithm; local custody infers it from key material.
    /// @ts-mutable
    #[serde(rename = "coinbaseJwtAlgorithm")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub coinbase_jwt_algorithm: Option<CoinbaseJwtAlgorithm>,
    /// @ts-mutable
    #[serde(rename = "tealstreetProxy")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_proxy: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "tealstreetTunnel")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_tunnel: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "tealstreetWsRoute")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_ws_route: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "proxyRegion")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub proxy_region: Option<String>,
    /// @ts-mutable
    #[serde(rename = "hyrotraderDemo")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub hyrotrader_demo: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "useExtensionDirect")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub use_extension_direct: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "user")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user: Option<String>,
    /// @ts-mutable
    #[serde(rename = "color")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub color: Option<String>,
    /// @ts-mutable
    #[serde(rename = "active")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub active: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "pinned")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub pinned: Option<bool>,
    /// @ts-mutable
    #[serde(rename = "telegramBotToken")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub telegram_bot_token: Option<String>,
    /// @ts-mutable
    #[serde(rename = "lastSymbol")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_symbol: Option<String>,
    /// @ts-mutable
    #[serde(rename = "healthStatus")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub health_status: Option<ExchangeAccountConfigHealthStatus>,
    /// @ts-mutable
    #[serde(rename = "updatedAt")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_at: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "deletedAt")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub deleted_at: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "syncId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub sync_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsAuth {
    /// @ts-mutable
    #[serde(rename = "userAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub user_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "signerAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub signer_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "privateKey")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub private_key: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsHyperliquidBuilder {
    #[serde(rename = "address")]
    pub address: String,
    #[serde(rename = "feeTenthsBps")]
    pub fee_tenths_bps: f64,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsBybitSiteId {
    #[serde(rename = "BRA_BTL")]
    BraBtl,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsTuning {
    #[serde(rename = "activeTickerSymbols")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub active_ticker_symbols: Option<Vec<String>>,
    #[serde(rename = "calculateUpnlFromRealtimeLastPrice")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub calculate_upnl_from_realtime_last_price: Option<bool>,
    #[serde(rename = "dataUpdateThrottleMultiplier")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub data_update_throttle_multiplier: Option<f64>,
    #[serde(rename = "skipRendersWhenWindowIsHidden")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub skip_renders_when_window_is_hidden: Option<bool>,
    #[serde(rename = "consoleLogging")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub console_logging: Option<bool>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsExtraAsterdexApiVersion {
    #[serde(rename = "v1")]
    V1,
    #[serde(rename = "v3")]
    V3,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsExtraLighterBuilderFeeMode {
    #[serde(rename = "dynamic")]
    Dynamic,
    #[serde(rename = "static")]
    Static,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsExtraKucoinBroker {
    #[serde(rename = "partner")]
    pub partner: String,
    #[serde(rename = "brokerKey")]
    pub broker_key: String,
    #[serde(rename = "brokerName")]
    pub broker_name: String,
    #[serde(rename = "verify")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub verify: Option<bool>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsExtraHyrotraderMode {
    #[serde(rename = "demo")]
    Demo,
    #[serde(rename = "production")]
    Production,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsExtra {
    /// @ts-mutable
    #[serde(rename = "tickInterval")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tick_interval: Option<f64>,
    /// @ts-mutable
    #[serde(
        rename = "rustCommandTimeoutMs",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rust_command_timeout_ms: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "recvWindow")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub recv_window: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "asterdexApiVersion")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub asterdex_api_version: Option<ExchangeOptionsExtraAsterdexApiVersion>,
    /// @ts-mutable
    #[serde(rename = "asterBuilderAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub aster_builder_address: Option<String>,
    /// @ts-mutable
    #[serde(rename = "asterBuilderFeeRateMaker")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub aster_builder_fee_rate_maker: Option<String>,
    /// @ts-mutable
    #[serde(rename = "asterBuilderFeeRateTaker")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub aster_builder_fee_rate_taker: Option<String>,
    /// @ts-mutable
    #[serde(rename = "lighterBuilderFeeMode")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub lighter_builder_fee_mode: Option<ExchangeOptionsExtraLighterBuilderFeeMode>,
    /// @ts-mutable
    #[serde(rename = "lighterIntegratorAccountIndex")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub lighter_integrator_account_index: Option<String>,
    /// @ts-mutable
    #[serde(rename = "lighterStopSlippagePct")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub lighter_stop_slippage_pct: Option<f64>,
    /// @ts-mutable
    #[serde(rename = "hyrotraderMode")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub hyrotrader_mode: Option<ExchangeOptionsExtraHyrotraderMode>,
    /// @ts-mutable
    #[serde(rename = "kucoinBroker")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub kucoin_broker: Option<ExchangeOptionsExtraKucoinBroker>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsExternalPublicDataVariant0Type {
    #[serde(rename = "websocket")]
    Websocket,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsExternalPublicDataVariant0 {
    /// @ts-mutable
    #[serde(rename = "type")]
    #[ts(inline)]
    pub r#type: ExchangeOptionsExternalPublicDataVariant0Type,
    /// @ts-mutable
    #[serde(rename = "url")]
    pub url: String,
    /// @ts-mutable
    #[serde(rename = "exchangeId")]
    pub exchange_id: String,
    /// @ts-mutable
    #[serde(rename = "authToken")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub auth_token: Option<String>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsExternalPublicDataVariant1Type {
    #[serde(rename = "in-process")]
    InProcess,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptionsExternalPublicDataVariant1 {
    /// @ts-mutable
    #[serde(rename = "type")]
    #[ts(inline)]
    pub r#type: ExchangeOptionsExternalPublicDataVariant1Type,
    /// @ts-mutable
    #[serde(rename = "source")]
    #[ts(type = "PublicDataSource")]
    pub source: PublicDataSourceCapability,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ExchangeOptionsExternalPublicData {
    Variant0(#[ts(inline)] ExchangeOptionsExternalPublicDataVariant0),
    Variant1(#[ts(inline)] ExchangeOptionsExternalPublicDataVariant1),
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptions {
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "secret")]
    pub secret: String,
    /// Account-worker mode; absent keeps existing signing selection.
    #[serde(rename = "signingMode")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub signing_mode: Option<SigningMode>,
    #[serde(rename = "passphrase")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub passphrase: Option<String>,
    #[serde(rename = "secondary_key")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_key: Option<String>,
    #[serde(rename = "secondary_secret")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_secret: Option<String>,
    #[serde(rename = "secondary_passphrase")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_passphrase: Option<String>,
    #[serde(rename = "secondaryProxyUrl")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secondary_proxy_url: Option<String>,
    #[serde(rename = "auth")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub auth: Option<ExchangeOptionsAuth>,
    #[serde(rename = "paradexAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub paradex_address: Option<String>,
    #[serde(rename = "subkeyAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub subkey_address: Option<String>,
    #[serde(rename = "grvtSignerAddress")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub grvt_signer_address: Option<String>,
    #[serde(rename = "grvtTradingAccountId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub grvt_trading_account_id: Option<String>,
    /// Hosted CDP key algorithm; local custody infers it from key material.
    #[serde(rename = "coinbaseJwtAlgorithm")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub coinbase_jwt_algorithm: Option<CoinbaseJwtAlgorithm>,
    #[serde(rename = "starkPrivateKey")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub stark_private_key: Option<String>,
    #[serde(rename = "vault")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub vault: Option<String>,
    #[serde(rename = "applicationId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub application_id: Option<String>,
    #[serde(rename = "apiKeyIndex")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key_index: Option<String>,
    #[serde(rename = "hyperliquidBuilder")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub hyperliquid_builder: Option<ExchangeOptionsHyperliquidBuilder>,
    #[serde(rename = "testnet")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub testnet: Option<bool>,
    #[serde(rename = "enableSpotMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_spot_markets: Option<bool>,
    #[serde(rename = "enableOptionMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_option_markets: Option<bool>,
    #[serde(rename = "enableInverseMarkets")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub enable_inverse_markets: Option<bool>,
    #[serde(rename = "bybitSiteId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub bybit_site_id: Option<ExchangeOptionsBybitSiteId>,
    #[serde(rename = "publicDataOnly")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub public_data_only: Option<bool>,
    #[serde(rename = "privateOnly")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub private_only: Option<bool>,
    #[serde(rename = "tealstreetProxy")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_proxy: Option<bool>,
    #[serde(rename = "tealstreetTunnel")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_tunnel: Option<bool>,
    #[serde(rename = "tealstreetWsRoute")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tealstreet_ws_route: Option<bool>,
    #[serde(rename = "overrideProxyUrl")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub override_proxy_url: Option<String>,
    #[serde(rename = "proxyRegion")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub proxy_region: Option<String>,
    #[serde(rename = "omniKeySeed")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub omni_key_seed: Option<String>,
    #[serde(rename = "subaccount")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub subaccount: Option<String>,
    #[serde(rename = "orderlyExchangeId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub orderly_exchange_id: Option<String>,
    #[serde(rename = "tuning")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub tuning: Option<ExchangeOptionsTuning>,
    #[serde(rename = "extra")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub extra: Option<ExchangeOptionsExtra>,
    /// Typed extension consumed by the Polymarket source constructor.
    #[serde(rename = "polymarket")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub polymarket: Option<ExchangeOptionsPolymarket>,
    #[serde(rename = "remoteSigner")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "RemoteSigner", optional)]
    pub remote_signer: Option<RemoteSignerCapability>,
    #[serde(rename = "transport")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "TransportProvider", optional)]
    pub transport: Option<TransportCapability>,
    #[serde(rename = "payloadSigner")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "PayloadSigner", optional)]
    pub payload_signer: Option<PayloadSignerCapability>,
    #[serde(rename = "hostedMode")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub hosted_mode: Option<bool>,
    #[serde(rename = "externalPublicData")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub external_public_data: Option<ExchangeOptionsExternalPublicData>,
    #[serde(
        rename = "localSigner",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "LocalSignerMetadata", optional)]
    pub local_signer: Option<LocalSignerMetadataCapability>,
    #[serde(
        rename = "hyperliquidFastBook",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub hyperliquid_fast_book: Option<bool>,
    #[serde(
        rename = "callerReportsPlacementFailures",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub caller_reports_placement_failures: Option<bool>,
    #[serde(
        rename = "workerMirror",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub worker_mirror: Option<bool>,
    #[serde(
        rename = "engine",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub engine: Option<ExchangeOptionsEngine>,
    #[serde(
        rename = "publicFeed",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "RustPublicFeedPolicy", optional)]
    pub public_feed: Option<RustPublicFeedCapability>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeAccount {
    #[serde(rename = "userId")]
    pub user_id: String,
    #[serde(rename = "subId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "string | null", optional)]
    pub sub_id: Option<Option<String>>,
    #[serde(rename = "affiliateId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "string | null", optional)]
    pub affiliate_id: Option<Option<String>>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeOptionsEngine {
    #[serde(rename = "rust")]
    Rust,
}
/// Typed account extension consumed by the Polymarket source constructor.
/// Unlike the venue's retained UTF-16 record, this uses the shared account
/// UTF-8 string boundary. signerType retains source numeric values verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct ExchangeOptionsPolymarket {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub market_slugs: Option<Vec<String>>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_key: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_secret: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub api_passphrase: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(type = "0 | 1 | 2 | 3", optional)]
    pub signer_type: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub funder_address: Option<String>,
}

mod strict_codecs {
    use super::*;
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::SigningMode")]
    enum WireSigningMode {
        #[serde(rename = "local")]
        Local,
        #[serde(rename = "cosigned")]
        Cosigned,
        #[serde(rename = "delegated")]
        Delegated,
    }
    impl<'de> Deserialize<'de> for SigningMode {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireSigningMode::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                String::deserialize(deserializer)?,
            ))
        }
    }
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::CoinbaseJwtAlgorithm")]
    enum WireCoinbaseJwtAlgorithm {
        #[serde(rename = "es256")]
        Es256,
        #[serde(rename = "edDsa")]
        EdDsa,
    }
    impl<'de> Deserialize<'de> for CoinbaseJwtAlgorithm {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireCoinbaseJwtAlgorithm::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(String::deserialize(
                    deserializer,
                )?),
            )
        }
    }
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsEngine")]
    enum WireExchangeOptionsEngine {
        #[serde(rename = "rust")]
        Rust,
    }
    impl<'de> Deserialize<'de> for ExchangeOptionsEngine {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsEngine::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(String::deserialize(
                    deserializer,
                )?),
            )
        }
    }
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccountConfigBybitSiteId")]
    enum WireExchangeAccountConfigBybitSiteId {
        #[serde(rename = "BRA_BTL")]
        BraBtl,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccountConfigBybitSiteId {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeAccountConfigBybitSiteId::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccountConfigHealthStatus")]
    enum WireExchangeAccountConfigHealthStatus {
        #[serde(rename = "ok")]
        Ok,
        #[serde(rename = "auth_error")]
        AuthError,
        #[serde(rename = "auth_expired")]
        AuthExpired,
        #[serde(rename = "permissions_error")]
        PermissionsError,
        #[serde(rename = "startup_failed")]
        StartupFailed,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccountConfigHealthStatus {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeAccountConfigHealthStatus::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccountConfig")]
    struct WireExchangeAccountConfig {
        #[serde(rename = "exchange")]
        exchange: ExchangeName,
        #[serde(rename = "name")]
        name: String,
        #[serde(rename = "testnet")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        testnet: Option<bool>,
        #[serde(rename = "apiKey")]
        api_key: String,
        #[serde(rename = "apiSecret")]
        api_secret: String,
        #[serde(rename = "signingMode")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        signing_mode: Option<SigningMode>,
        #[serde(rename = "passphrase")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        passphrase: Option<String>,
        #[serde(rename = "secondary_apiKey")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_api_key: Option<String>,
        #[serde(rename = "secondary_apiSecret")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_api_secret: Option<String>,
        #[serde(rename = "secondary_passphrase")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_passphrase: Option<String>,
        #[serde(rename = "enableSpotMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_spot_markets: Option<bool>,
        #[serde(rename = "enableOptionMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_option_markets: Option<bool>,
        #[serde(rename = "enableInverseMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_inverse_markets: Option<bool>,
        #[serde(rename = "bybitSiteId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        bybit_site_id: Option<ExchangeAccountConfigBybitSiteId>,
        #[serde(rename = "applicationId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        application_id: Option<String>,
        #[serde(rename = "apiKeyIndex")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        api_key_index: Option<String>,
        #[serde(rename = "vault")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        vault: Option<String>,
        #[serde(rename = "omniKeySeed")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        omni_key_seed: Option<String>,
        #[serde(rename = "subaccount")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        subaccount: Option<String>,
        #[serde(rename = "orderlyExchangeId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        orderly_exchange_id: Option<String>,
        #[serde(rename = "paradexAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        paradex_address: Option<String>,
        #[serde(rename = "subkeyAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        subkey_address: Option<String>,
        #[serde(rename = "grvtSignerAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        grvt_signer_address: Option<String>,
        #[serde(rename = "grvtTradingAccountId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        grvt_trading_account_id: Option<String>,
        #[serde(rename = "coinbaseJwtAlgorithm")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        coinbase_jwt_algorithm: Option<CoinbaseJwtAlgorithm>,
        #[serde(rename = "tealstreetProxy")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_proxy: Option<bool>,
        #[serde(rename = "tealstreetTunnel")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_tunnel: Option<bool>,
        #[serde(rename = "tealstreetWsRoute")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_ws_route: Option<bool>,
        #[serde(rename = "proxyRegion")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        proxy_region: Option<String>,
        #[serde(rename = "hyrotraderDemo")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        hyrotrader_demo: Option<bool>,
        #[serde(rename = "useExtensionDirect")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        use_extension_direct: Option<bool>,
        #[serde(rename = "user")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        user: Option<String>,
        #[serde(rename = "color")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        color: Option<String>,
        #[serde(rename = "active")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        active: Option<bool>,
        #[serde(rename = "pinned")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        pinned: Option<bool>,
        #[serde(rename = "telegramBotToken")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        telegram_bot_token: Option<String>,
        #[serde(rename = "lastSymbol")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        last_symbol: Option<String>,
        #[serde(rename = "healthStatus")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        health_status: Option<ExchangeAccountConfigHealthStatus>,
        #[serde(rename = "updatedAt")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        updated_at: Option<f64>,
        #[serde(rename = "deletedAt")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        deleted_at: Option<f64>,
        #[serde(rename = "syncId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        sync_id: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccountConfig {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeAccountConfig::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsAuth")]
    struct WireExchangeOptionsAuth {
        #[serde(rename = "userAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        user_address: Option<String>,
        #[serde(rename = "signerAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        signer_address: Option<String>,
        #[serde(rename = "privateKey")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        private_key: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsAuth {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsAuth::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsHyperliquidBuilder")]
    struct WireExchangeOptionsHyperliquidBuilder {
        #[serde(rename = "address")]
        address: String,
        #[serde(rename = "feeTenthsBps")]
        fee_tenths_bps: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsHyperliquidBuilder {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsHyperliquidBuilder::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsBybitSiteId")]
    enum WireExchangeOptionsBybitSiteId {
        #[serde(rename = "BRA_BTL")]
        BraBtl,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsBybitSiteId {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsBybitSiteId::deserialize(serde::de::value::StringDeserializer::<
                D::Error,
            >::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsTuning")]
    struct WireExchangeOptionsTuning {
        #[serde(rename = "activeTickerSymbols")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        active_ticker_symbols: Option<Vec<String>>,
        #[serde(rename = "calculateUpnlFromRealtimeLastPrice")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        calculate_upnl_from_realtime_last_price: Option<bool>,
        #[serde(rename = "dataUpdateThrottleMultiplier")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        data_update_throttle_multiplier: Option<f64>,
        #[serde(rename = "skipRendersWhenWindowIsHidden")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        skip_renders_when_window_is_hidden: Option<bool>,
        #[serde(rename = "consoleLogging")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        console_logging: Option<bool>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsTuning {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsTuning::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExtraAsterdexApiVersion")]
    enum WireExchangeOptionsExtraAsterdexApiVersion {
        #[serde(rename = "v1")]
        V1,
        #[serde(rename = "v3")]
        V3,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExtraAsterdexApiVersion {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsExtraAsterdexApiVersion::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExtraLighterBuilderFeeMode")]
    enum WireExchangeOptionsExtraLighterBuilderFeeMode {
        #[serde(rename = "dynamic")]
        Dynamic,
        #[serde(rename = "static")]
        Static,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExtraLighterBuilderFeeMode {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsExtraLighterBuilderFeeMode::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExtraKucoinBroker")]
    struct WireExchangeOptionsExtraKucoinBroker {
        #[serde(rename = "partner")]
        partner: String,
        #[serde(rename = "brokerKey")]
        broker_key: String,
        #[serde(rename = "brokerName")]
        broker_name: String,
        #[serde(rename = "verify")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        verify: Option<bool>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExtraKucoinBroker {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsExtraKucoinBroker::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExtraHyrotraderMode")]
    enum WireExchangeOptionsExtraHyrotraderMode {
        #[serde(rename = "demo")]
        Demo,
        #[serde(rename = "production")]
        Production,
    }
    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExtraHyrotraderMode {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsExtraHyrotraderMode::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExtra")]
    struct WireExchangeOptionsExtra {
        #[serde(rename = "tickInterval")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tick_interval: Option<f64>,
        #[serde(
            rename = "rustCommandTimeoutMs",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        rust_command_timeout_ms: Option<f64>,
        #[serde(rename = "recvWindow")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        recv_window: Option<f64>,
        #[serde(rename = "asterdexApiVersion")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        asterdex_api_version: Option<ExchangeOptionsExtraAsterdexApiVersion>,
        #[serde(rename = "asterBuilderAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        aster_builder_address: Option<String>,
        #[serde(rename = "asterBuilderFeeRateMaker")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        aster_builder_fee_rate_maker: Option<String>,
        #[serde(rename = "asterBuilderFeeRateTaker")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        aster_builder_fee_rate_taker: Option<String>,
        #[serde(rename = "lighterBuilderFeeMode")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        lighter_builder_fee_mode: Option<ExchangeOptionsExtraLighterBuilderFeeMode>,
        #[serde(rename = "lighterIntegratorAccountIndex")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        lighter_integrator_account_index: Option<String>,
        #[serde(rename = "lighterStopSlippagePct")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        lighter_stop_slippage_pct: Option<f64>,
        #[serde(rename = "hyrotraderMode")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        hyrotrader_mode: Option<ExchangeOptionsExtraHyrotraderMode>,
        #[serde(rename = "kucoinBroker")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        kucoin_broker: Option<ExchangeOptionsExtraKucoinBroker>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExtra {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsExtra::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExternalPublicDataVariant0Type")]
    enum WireExchangeOptionsExternalPublicDataVariant0Type {
        #[serde(rename = "websocket")]
        Websocket,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExternalPublicDataVariant0Type {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsExternalPublicDataVariant0Type::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExternalPublicDataVariant0")]
    struct WireExchangeOptionsExternalPublicDataVariant0 {
        #[serde(rename = "type")]
        r#type: ExchangeOptionsExternalPublicDataVariant0Type,
        #[serde(rename = "url")]
        url: String,
        #[serde(rename = "exchangeId")]
        exchange_id: String,
        #[serde(rename = "authToken")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        auth_token: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExternalPublicDataVariant0 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsExternalPublicDataVariant0::deserialize(
                crate::strict_decode::object(deserializer),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExternalPublicDataVariant1Type")]
    enum WireExchangeOptionsExternalPublicDataVariant1Type {
        #[serde(rename = "in-process")]
        InProcess,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExternalPublicDataVariant1Type {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeOptionsExternalPublicDataVariant1Type::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsExternalPublicDataVariant1")]
    struct WireExchangeOptionsExternalPublicDataVariant1 {
        #[serde(rename = "type")]
        r#type: ExchangeOptionsExternalPublicDataVariant1Type,
        #[serde(rename = "source")]
        source: PublicDataSourceCapability,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsExternalPublicDataVariant1 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsExternalPublicDataVariant1::deserialize(
                crate::strict_decode::object(deserializer),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptions")]
    struct WireExchangeOptions {
        #[serde(rename = "key")]
        key: String,
        #[serde(rename = "secret")]
        secret: String,
        #[serde(rename = "signingMode")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        signing_mode: Option<SigningMode>,
        #[serde(rename = "passphrase")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        passphrase: Option<String>,
        #[serde(rename = "secondary_key")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_key: Option<String>,
        #[serde(rename = "secondary_secret")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_secret: Option<String>,
        #[serde(rename = "secondary_passphrase")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_passphrase: Option<String>,
        #[serde(rename = "secondaryProxyUrl")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secondary_proxy_url: Option<String>,
        #[serde(rename = "auth")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        auth: Option<ExchangeOptionsAuth>,
        #[serde(rename = "paradexAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        paradex_address: Option<String>,
        #[serde(rename = "subkeyAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        subkey_address: Option<String>,
        #[serde(rename = "grvtSignerAddress")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        grvt_signer_address: Option<String>,
        #[serde(rename = "grvtTradingAccountId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        grvt_trading_account_id: Option<String>,
        #[serde(rename = "coinbaseJwtAlgorithm")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        coinbase_jwt_algorithm: Option<CoinbaseJwtAlgorithm>,
        #[serde(rename = "starkPrivateKey")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        stark_private_key: Option<String>,
        #[serde(rename = "vault")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        vault: Option<String>,
        #[serde(rename = "applicationId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        application_id: Option<String>,
        #[serde(rename = "apiKeyIndex")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        api_key_index: Option<String>,
        #[serde(rename = "hyperliquidBuilder")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        hyperliquid_builder: Option<ExchangeOptionsHyperliquidBuilder>,
        #[serde(rename = "testnet")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        testnet: Option<bool>,
        #[serde(rename = "enableSpotMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_spot_markets: Option<bool>,
        #[serde(rename = "enableOptionMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_option_markets: Option<bool>,
        #[serde(rename = "enableInverseMarkets")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        enable_inverse_markets: Option<bool>,
        #[serde(rename = "bybitSiteId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        bybit_site_id: Option<ExchangeOptionsBybitSiteId>,
        #[serde(rename = "publicDataOnly")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        public_data_only: Option<bool>,
        #[serde(rename = "privateOnly")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        private_only: Option<bool>,
        #[serde(rename = "tealstreetProxy")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_proxy: Option<bool>,
        #[serde(rename = "tealstreetTunnel")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_tunnel: Option<bool>,
        #[serde(rename = "tealstreetWsRoute")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tealstreet_ws_route: Option<bool>,
        #[serde(rename = "overrideProxyUrl")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        override_proxy_url: Option<String>,
        #[serde(rename = "proxyRegion")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        proxy_region: Option<String>,
        #[serde(rename = "omniKeySeed")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        omni_key_seed: Option<String>,
        #[serde(rename = "subaccount")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        subaccount: Option<String>,
        #[serde(rename = "orderlyExchangeId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        orderly_exchange_id: Option<String>,
        #[serde(rename = "tuning")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tuning: Option<ExchangeOptionsTuning>,
        #[serde(rename = "extra")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        extra: Option<ExchangeOptionsExtra>,
        #[serde(rename = "polymarket")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        polymarket: Option<super::ExchangeOptionsPolymarket>,
        #[serde(rename = "remoteSigner")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        remote_signer: Option<RemoteSignerCapability>,
        #[serde(rename = "transport")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        transport: Option<TransportCapability>,
        #[serde(rename = "payloadSigner")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        payload_signer: Option<PayloadSignerCapability>,
        #[serde(rename = "hostedMode")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        hosted_mode: Option<bool>,
        #[serde(rename = "externalPublicData")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        external_public_data: Option<ExchangeOptionsExternalPublicData>,
        #[serde(
            rename = "localSigner",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        local_signer: Option<LocalSignerMetadataCapability>,
        #[serde(
            rename = "hyperliquidFastBook",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        hyperliquid_fast_book: Option<bool>,
        #[serde(
            rename = "callerReportsPlacementFailures",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        caller_reports_placement_failures: Option<bool>,
        #[serde(
            rename = "workerMirror",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        worker_mirror: Option<bool>,
        #[serde(
            rename = "engine",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        engine: Option<ExchangeOptionsEngine>,
        #[serde(
            rename = "publicFeed",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        public_feed: Option<RustPublicFeedCapability>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeAccount")]
    struct WireExchangeAccount {
        #[serde(rename = "userId")]
        user_id: String,
        #[serde(rename = "subId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        sub_id: Option<Option<String>>,
        #[serde(rename = "affiliateId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        affiliate_id: Option<Option<String>>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeAccount {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeAccount::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeOptionsPolymarket")]
    #[serde(rename_all = "camelCase")]
    struct WireExchangeOptionsPolymarket {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        market_slugs: Option<Vec<String>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        api_key: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        api_secret: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        api_passphrase: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        signer_type: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        funder_address: Option<String>,
    }
    impl<'de> serde::Deserialize<'de> for super::ExchangeOptionsPolymarket {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptionsPolymarket::deserialize(crate::strict_decode::object(deserializer))
        }
    }
    #[cfg(test)]
    mod tests {
        use super::*;
        #[test]
        fn account_options_keep_strict_record_and_literal_guards() {
            crate::strict_decode::tests::string_union::<ExchangeAccountConfigBybitSiteId>("ExchangeAccountConfigBybitSiteId", WireExchangeAccountConfigBybitSiteId::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeAccountConfigBybitSiteId::BraBtl).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::string_union::<ExchangeAccountConfigHealthStatus>("ExchangeAccountConfigHealthStatus", WireExchangeAccountConfigHealthStatus::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeAccountConfigHealthStatus::Ok).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountConfigHealthStatus::AuthError).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountConfigHealthStatus::AuthExpired).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountConfigHealthStatus::PermissionsError).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeAccountConfigHealthStatus::StartupFailed).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeAccountConfig>("ExchangeAccountConfig");
            crate::strict_decode::tests::record::<ExchangeOptionsAuth>("ExchangeOptionsAuth");
            crate::strict_decode::tests::record::<ExchangeOptionsHyperliquidBuilder>(
                "ExchangeOptionsHyperliquidBuilder",
            );
            crate::strict_decode::tests::string_union::<ExchangeOptionsBybitSiteId>("ExchangeOptionsBybitSiteId", WireExchangeOptionsBybitSiteId::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsBybitSiteId::BraBtl).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeOptionsTuning>("ExchangeOptionsTuning");
            crate::strict_decode::tests::string_union::<ExchangeOptionsExtraAsterdexApiVersion>("ExchangeOptionsExtraAsterdexApiVersion", WireExchangeOptionsExtraAsterdexApiVersion::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsExtraAsterdexApiVersion::V1).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeOptionsExtraAsterdexApiVersion::V3).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::string_union::<ExchangeOptionsExtraLighterBuilderFeeMode>("ExchangeOptionsExtraLighterBuilderFeeMode", WireExchangeOptionsExtraLighterBuilderFeeMode::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsExtraLighterBuilderFeeMode::Dynamic).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeOptionsExtraLighterBuilderFeeMode::Static).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeOptionsExtraKucoinBroker>(
                "ExchangeOptionsExtraKucoinBroker",
            );
            crate::strict_decode::tests::string_union::<ExchangeOptionsExtraHyrotraderMode>("ExchangeOptionsExtraHyrotraderMode", WireExchangeOptionsExtraHyrotraderMode::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsExtraHyrotraderMode::Demo).unwrap().as_str().unwrap().to_owned(), serde_json::to_value(ExchangeOptionsExtraHyrotraderMode::Production).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeOptionsExtra>("ExchangeOptionsExtra");
            crate::strict_decode::tests::string_union::<ExchangeOptionsExternalPublicDataVariant0Type>("ExchangeOptionsExternalPublicDataVariant0Type", WireExchangeOptionsExternalPublicDataVariant0Type::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsExternalPublicDataVariant0Type::Websocket).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeOptionsExternalPublicDataVariant0>(
                "ExchangeOptionsExternalPublicDataVariant0",
            );
            crate::strict_decode::tests::string_union::<ExchangeOptionsExternalPublicDataVariant1Type>("ExchangeOptionsExternalPublicDataVariant1Type", WireExchangeOptionsExternalPublicDataVariant1Type::deserialize(serde::de::value::StringDeserializer::<crate::strict_decode::tests::Inventory>::new("__unknown_inventory__".into())).err().unwrap().0, vec![serde_json::to_value(ExchangeOptionsExternalPublicDataVariant1Type::InProcess).unwrap().as_str().unwrap().to_owned()]);
            crate::strict_decode::tests::record::<ExchangeOptionsExternalPublicDataVariant1>(
                "ExchangeOptionsExternalPublicDataVariant1",
            );
            crate::strict_decode::tests::record::<ExchangeOptionsPolymarket>(
                "ExchangeOptionsPolymarket",
            );
            crate::strict_decode::tests::string_union::<ExchangeOptionsEngine>(
                "ExchangeOptionsEngine",
                WireExchangeOptionsEngine::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec!["rust".into()],
            );
            crate::strict_decode::tests::record::<ExchangeOptions>("ExchangeOptions");
            crate::strict_decode::tests::record::<ExchangeAccount>("ExchangeAccount");
        }
    }
}
