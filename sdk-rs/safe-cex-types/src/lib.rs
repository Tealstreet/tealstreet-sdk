//! Rust-owned finite JSON domain models. See ../TYPE_PARITY.md for generator compatibility limits.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

pub mod options;
pub use options::{
    ExchangeAccount, ExchangeAccountConfig, ExchangeAccountConfigBybitSiteId,
    ExchangeAccountConfigHealthStatus, ExchangeOptions, ExchangeOptionsAuth,
    ExchangeOptionsBybitSiteId, ExchangeOptionsEngine, ExchangeOptionsExternalPublicData,
    ExchangeOptionsExternalPublicDataVariant0, ExchangeOptionsExternalPublicDataVariant0Type,
    ExchangeOptionsExternalPublicDataVariant1, ExchangeOptionsExternalPublicDataVariant1Type,
    ExchangeOptionsExtra, ExchangeOptionsExtraAsterdexApiVersion,
    ExchangeOptionsExtraHyrotraderMode, ExchangeOptionsExtraKucoinBroker,
    ExchangeOptionsExtraLighterBuilderFeeMode, ExchangeOptionsHyperliquidBuilder,
    ExchangeOptionsPolymarket, ExchangeOptionsTuning, LocalSignerMetadataCapability,
    RustPublicFeedCapability,
};

pub mod history;
pub use history::{
    ClosedPnlEntry, ClosedPnlEntryContracts, ClosedPnlEntryEntryPrice, ClosedPnlEntryExitPrice,
    ClosedPnlEntryFees, ClosedPnlEntryLeverage, ClosedPnlEntryNotional,
    ClosedPnlEntryUnrealizedPnl, DataProvenance, HistoricalOrder, HistoricalTrade, IncomeCategory,
    IncomeRecord,
};

pub mod order;
pub use order::{
    Order, OrderExtra, OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType,
    OrderTimeInForce, OrderType, PlaceOrderOpts, UpdateOrderOpts, UpdateOrderOptsUpdate,
    UpdateOrderOptsUpdateVariant0, UpdateOrderOptsUpdateVariant1,
};

pub mod market_data;
pub use market_data::{Candle, OrderBook, OrderBookOrders};

pub mod position;
pub(crate) use position::serialize_position_fields;
pub use position::{Position, PositionExtra, PositionOption};

mod execution;
pub use execution::{Execution, OrderFillEvent, PublicTrade};
pub mod model_field;
pub use model_field::ModelField;
mod info_record;
pub use info_record::STORE_RECORD;
pub mod query_value;
pub mod retained_json;
pub mod store_access;
pub mod telemetry;
pub use query_value::QueryValue;
pub use store_access::{StoreReadError, StoreReadPhase};
pub mod validation_error;
pub use validation_error::AccountValidationError;

// Missing optional fields default to None; present null is valid only for info:any.
fn deserialize_present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum PositionSide {
    #[serde(rename = "long")]
    Long,
    #[serde(rename = "short")]
    Short,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum PositionMode {
    #[serde(rename = "hedged")]
    Hedged,
    #[serde(rename = "oneway")]
    OneWay,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum MarginType {
    #[serde(rename = "isolated")]
    Isolated,
    #[serde(rename = "cross")]
    Cross,
}

/// @ts-union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OptionRight {
    #[serde(rename = "call")]
    Call,
    #[serde(rename = "put")]
    Put,
}

/// @ts-union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OptionTermsStyle {
    #[serde(rename = "european")]
    European,
}

/// @ts-union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OptionTermsSettlement {
    #[serde(rename = "cash")]
    Cash,
}

/// What makes a market an option. Absent on every other market.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OptionTerms {
    pub underlying: String,
    /// Expiry, ms since epoch.
    pub expiry: f64,
    pub strike: f64,
    pub right: OptionRight,
    #[ts(inline)]
    pub style: OptionTermsStyle,
    #[ts(inline)]
    pub settlement: OptionTermsSettlement,
    /// The asset the premium is quoted in, which is the unit of `price` on an option order.
    pub premium_asset: String,
    pub settle_asset: String,
}

/// Greeks as the venue publishes them; `units` records what was measured.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OptionGreeks {
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub units: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct MarketPrecision {
    pub amount: f64,
    pub price: f64,
    // Runtime properties emitted by maintained venue mappers (outside the TS declaration).
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub base: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub quote: Option<f64>,
}

/// A numeric runtime property whose source mapper may retain numeric text.
/// Arithmetic consumers explicitly apply JavaScript Number conversion in num.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum NumericValue {
    Number(f64),
    Text(String),
}
impl From<f64> for NumericValue {
    fn from(value: f64) -> Self {
        Self::Number(value)
    }
}
impl From<String> for NumericValue {
    fn from(value: String) -> Self {
        Self::Text(value)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct MarketLimitsAmount {
    pub min: f64,
    #[ts(type = "number")]
    pub max: NumericValue,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct MarketLimitsLeverage {
    pub min: f64,
    pub max: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct MarketLimitsPrice {
    pub min: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct MarketLimits {
    #[ts(inline)]
    pub amount: MarketLimitsAmount,
    #[ts(inline)]
    pub leverage: MarketLimitsLeverage,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub price: Option<MarketLimitsPrice>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub cost: Option<MarketLimitsLeverage>,
}

/// Runtime fee object constructed by venue market mappers.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct MarketFees {
    pub maker: f64,
    pub taker: f64,
}

/// Extra properties constructed by venue TypeScript at runtime, beyond Market's declaration.
#[derive(Debug, Clone, PartialEq, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MarketRuntime {
    #[serde(
        rename = "type",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub kind: Option<String>,
    #[serde(
        rename = "spot",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub spot: Option<bool>,
    #[serde(
        rename = "swap",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub swap: Option<bool>,
    #[serde(
        rename = "future",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub future: Option<bool>,
    #[serde(
        rename = "contract",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub contract: Option<bool>,
    #[serde(
        rename = "linear",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub linear: Option<bool>,
    #[serde(
        rename = "inverse",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub inverse: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fees: Option<MarketFees>,
}

pub mod market;
pub use market::Market;

pub mod balance;
pub use balance::{Balance, CoinBalance, CurrencyBalance};

pub mod ticker;
pub use ticker::Ticker;

/// One option's prices and implied volatilities. IVs are fractions; 0 means unavailable.
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OptionQuote {
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub mark: f64,
    pub mark_iv: f64,
    pub bid_iv: f64,
    pub ask_iv: f64,
    pub underlying_price: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub forward_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub greeks: Option<OptionGreeks>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OptionChainQuery {
    pub underlying: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub expiry: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub right: Option<OptionRight>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub strike_min: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub strike_max: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OptionChainRow {
    pub market: Market,
    pub quote: OptionQuote,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct AccountGreeks {
    pub underlying: String,
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub units: String,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct HistoricalTradeOptions {
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from_id: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub account_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OHLCVOptions {
    pub symbol: String,
    // The TS declaration is narrower than the source worker's runtime interval domain.
    #[ts(as = "Timeframe")]
    pub interval: AnyTimeframe,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OHLCVSOptions {
    pub symbols: Vec<String>,
    pub interval: Timeframe,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct ExecutionsOptions {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderHistoryOptions {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub from: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub to: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub status: Option<Vec<OrderStatus>>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct IncomeHistoryOptions {
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub symbol: Option<String>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub income_type: Option<String>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub start_time: Option<f64>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub end_time: Option<f64>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub limit: Option<f64>,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ExecutionPositionSide {
    Side(PositionSide),
    #[ts(type = "'both'")]
    Both(BothPositionSide),
}

/// @ts-union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum BothPositionSide {
    #[serde(rename = "both")]
    Both,
}

/// @ts-union
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum Timeframe {
    #[serde(rename = "1m")]
    M1,
    #[serde(rename = "3m")]
    M3,
    #[serde(rename = "5m")]
    M5,
    #[serde(rename = "15m")]
    M15,
    #[serde(rename = "30m")]
    M30,
    #[serde(rename = "1h")]
    H1,
    #[serde(rename = "2h")]
    H2,
    #[serde(rename = "4h")]
    H4,
    #[serde(rename = "6h")]
    H6,
    #[serde(rename = "12h")]
    H12,
    #[serde(rename = "1d")]
    D1,
    #[serde(rename = "1w")]
    W1,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExchangeName {
    #[serde(rename = "apexomni")]
    Apexomni,
    #[serde(rename = "asterdex")]
    Asterdex,
    #[serde(rename = "backpack")]
    Backpack,
    #[serde(rename = "binance")]
    Binance,
    #[serde(rename = "binanceinverse")]
    Binanceinverse,
    #[serde(rename = "binanceoptions")]
    Binanceoptions,
    #[serde(rename = "binancespot")]
    Binancespot,
    #[serde(rename = "binancepm")]
    Binancepm,
    #[serde(rename = "bingx")]
    Bingx,
    #[serde(rename = "bingxspot")]
    Bingxspot,
    #[serde(rename = "bitget")]
    Bitget,
    #[serde(rename = "bitgetinverse")]
    Bitgetinverse,
    #[serde(rename = "bitgetusdc")]
    Bitgetusdc,
    #[serde(rename = "bitrue")]
    Bitrue,
    #[serde(rename = "bitunix")]
    Bitunix,
    #[serde(rename = "blofin")]
    Blofin,
    #[serde(rename = "bybit")]
    Bybit,
    #[serde(rename = "bybitdemo")]
    Bybitdemo,
    #[serde(rename = "bybitinverse")]
    Bybitinverse,
    #[serde(rename = "bybitspot")]
    Bybitspot,
    #[serde(rename = "hyrotrader")]
    Hyrotrader,
    #[serde(rename = "coinbase")]
    Coinbase,
    #[serde(rename = "coinbasespot")]
    Coinbasespot,
    #[serde(rename = "cryptocom")]
    Cryptocom,
    #[serde(rename = "grvt")]
    Grvt,
    #[serde(rename = "hyperliquid")]
    Hyperliquid,
    #[serde(rename = "hyperliquidspot")]
    Hyperliquidspot,
    #[serde(rename = "lighter")]
    Lighter,
    #[serde(rename = "lighter-robinhood")]
    LighterRobinhood,
    #[serde(rename = "mexc")]
    Mexc,
    #[serde(rename = "okx")]
    Okx,
    #[serde(rename = "okxdemo")]
    Okxdemo,
    #[serde(rename = "okxeu")]
    Okxeu,
    #[serde(rename = "okxeudemo")]
    Okxeudemo,
    #[serde(rename = "paradex")]
    Paradex,
    #[serde(rename = "phemex")]
    Phemex,
    #[serde(rename = "polymarket")]
    Polymarket,
    #[serde(rename = "kodiakfi")]
    Kodiakfi,
    #[serde(rename = "mode")]
    Mode,
    #[serde(rename = "txflow")]
    Txflow,
    #[serde(rename = "what")]
    What,
    #[serde(rename = "weex")]
    Weex,
    #[serde(rename = "woo")]
    Woo,
    #[serde(rename = "woofi")]
    Woofi,
    #[serde(rename = "kucoin")]
    Kucoin,
    #[serde(rename = "kucoinspot")]
    Kucoinspot,
    #[serde(rename = "kraken")]
    Kraken,
    #[serde(rename = "krakenspot")]
    Krakenspot,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ValidateAccountResult {
    /// @ts-mutable
    #[serde(rename = "error")]
    /// Static string projection of a declaration the runtime violates: truthy numeric/object errors survive.
    /// See parity/manifest/types.ts.toml::ValidateAccountResult (ts-bug: validation-error-value).
    #[ts(type = "string")]
    pub error: AccountValidationError,
    /// @ts-mutable
    #[serde(rename = "balance")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub balance: Option<Balance>,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum RequestOutcome {
    #[serde(rename = "ok")]
    Ok,
    #[serde(rename = "rateLimit")]
    RateLimit,
    #[serde(rename = "auth")]
    Auth,
    #[serde(rename = "network")]
    Network,
    #[serde(rename = "other")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(type = "Record<RequestOutcome, number>")]
pub struct RequestOutcomeCounts {
    pub ok: f64,
    #[serde(rename = "rateLimit")]
    pub rate_limit: f64,
    pub auth: f64,
    pub network: f64,
    pub other: f64,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum MessageHandlerStatsType {
    #[serde(rename = "public")]
    Public,
    #[serde(rename = "private")]
    Private,
    #[serde(rename = "futures")]
    Futures,
    #[serde(rename = "business")]
    Business,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum SecondsTimeframe {
    #[serde(rename = "1s")]
    S1,
    #[serde(rename = "5s")]
    S5,
    #[serde(rename = "15s")]
    S15,
    #[serde(rename = "30s")]
    S30,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum ExtendedTimeframe {
    #[serde(rename = "3h")]
    H3,
    #[serde(rename = "5h")]
    H5,
    #[serde(rename = "7h")]
    H7,
    #[serde(rename = "8h")]
    H8,
    #[serde(rename = "9h")]
    H9,
    #[serde(rename = "10h")]
    H10,
    #[serde(rename = "11h")]
    H11,
    #[serde(rename = "24h")]
    H24,
    #[serde(rename = "2d")]
    D2,
    #[serde(rename = "3d")]
    D3,
    #[serde(rename = "1M")]
    MN1,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum AnyTimeframe {
    Variant0(SecondsTimeframe),
    Variant1(Timeframe),
    Variant2(ExtendedTimeframe),
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct PublicExchangeOptions {
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
    #[serde(rename = "key")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub key: Option<String>,
    #[serde(rename = "secret")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub secret: Option<String>,
    #[serde(rename = "publicDataOnly")]
    #[ts(type = "true")]
    pub public_data_only: True,
    #[serde(rename = "forceAllProxiesOff")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub force_all_proxies_off: Option<bool>,
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
    #[serde(
        rename = "polymarket",
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub polymarket: Option<ExchangeOptionsPolymarket>,
}

// These distinct uninhabited markers cannot encode or invent an in-process capability.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum RemoteSignerCapability {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PayloadSignerCapability {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum TransportCapability {}
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum PublicDataSourceCapability {}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct True;
impl Serialize for True {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        serializer.serialize_bool(true)
    }
}
impl<'de> Deserialize<'de> for True {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if bool::deserialize(deserializer)? {
            Ok(Self)
        } else {
            Err(serde::de::Error::custom("expected true"))
        }
    }
}

// D1 generator source for types.ts's risk-engine result rows.
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct OrderRiskProfile {
    /// @ts-mutable
    pub is_looking_like_a_full_stop: Option<bool>,
    /// @ts-mutable
    pub order_pnl: f64,
    /// @ts-mutable
    pub total_realized_pnl: f64,
    /// @ts-mutable
    pub rolling_unrealized_pnl: f64,
    /// @ts-mutable
    #[ts(as = "PositionSide")]
    pub order_line_pos_side: ModelField<PositionSide>,
    /// @ts-mutable
    pub position_size: f64,
    /// @ts-mutable
    pub order_line_text: Option<String>,
    /// @ts-mutable
    pub fee: Option<f64>,
    /// Rust construction metadata; the loss-side limit branch orders size before fee.
    #[doc(hidden)]
    #[ts(skip)]
    pub position_size_before_fee: bool,
}
impl Serialize for OrderRiskProfile {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut row = serializer.serialize_struct(
            crate::model_field::MODEL_RECORD,
            5 + usize::from(self.is_looking_like_a_full_stop.is_some())
                + usize::from(self.order_line_text.is_some())
                + usize::from(self.fee.is_some()),
        )?;
        if let Some(value) = self.is_looking_like_a_full_stop {
            row.serialize_field("isLookingLikeAFullStop", &value)?;
        }
        row.serialize_field("orderPnl", &self.order_pnl)?;
        row.serialize_field("totalRealizedPnl", &self.total_realized_pnl)?;
        row.serialize_field("rollingUnrealizedPnl", &self.rolling_unrealized_pnl)?;
        self.order_line_pos_side
            .serialize_field(&mut row, "orderLinePosSide")?;
        if let Some(value) = &self.order_line_text {
            row.serialize_field("orderLineText", value)?;
        }
        if self.position_size_before_fee {
            row.serialize_field("positionSize", &self.position_size)?;
        }
        if let Some(value) = self.fee {
            row.serialize_field("fee", &value)?;
        }
        if !self.position_size_before_fee {
            row.serialize_field("positionSize", &self.position_size)?;
        }
        row.end()
    }
}

pub mod store_model;
pub use store_model::{
    ActiveChaser, ActiveTWAP, DisplayedMargin, DisplayedPosition, FetchedLeverage, StoreData,
    StoreDataLoaded, StoreOptions, StoreUpdateIds,
};

pub mod extra_model;
pub mod model_utils;
pub mod symbol_converter;
pub use extra_model::{
    PublicDataSource, PublicDataStore, READ_ONLY_ORDER_STATUSES, TIMEFRAME_TO_SECONDS, Writable,
    WritableStoreData,
};
// Shared gateway wire models are below venues; registry descriptors remain above.
pub mod accounts;
pub mod gateway;

pub mod js_coercion;
pub mod object_map;
pub mod strict_decode;

// Private wire companions keep serde metadata without exposing permissive decoders.
mod strict_codecs {
    #[cfg(test)]
    use super::CoinBalance;
    #[cfg(test)]
    use super::ExchangeName;
    #[cfg(test)]
    use super::OrderBookOrders;
    #[cfg(test)]
    use super::OrderFillEvent;
    #[cfg(test)]
    use super::{
        AccountGreeks, BothPositionSide, Candle, ClosedPnlEntry, Execution, ExecutionsOptions,
        ExtendedTimeframe, HistoricalOrder, HistoricalTrade, HistoricalTradeOptions,
        IncomeHistoryOptions, IncomeRecord, MarginType, MessageHandlerStatsType, OHLCVOptions,
        OHLCVSOptions, OptionChainQuery, OptionChainRow, OptionGreeks, OptionQuote, OrderBook,
        OrderHistoryOptions, OrderRiskProfile, PositionMode, PublicExchangeOptions, PublicTrade,
        RequestOutcome, RequestOutcomeCounts, SecondsTimeframe, Ticker, ValidateAccountResult,
    };
    use super::{
        AccountValidationError, AnyTimeframe, Balance, ExchangeOptionsAuth,
        ExchangeOptionsBybitSiteId, ExchangeOptionsExternalPublicData, ExchangeOptionsExtra,
        ExchangeOptionsHyperliquidBuilder, ExchangeOptionsTuning, Market, MarketLimits,
        MarketLimitsAmount, MarketLimitsLeverage, MarketLimitsPrice, MarketPrecision, ModelField,
        OptionRight, OptionTerms, OptionTermsSettlement, OptionTermsStyle, OrderStatus,
        PayloadSignerCapability, PositionSide, RemoteSignerCapability, Timeframe,
        TransportCapability, True, deserialize_present,
    };
    use super::{
        ExchangeOptionsEngine, ExchangeOptionsPolymarket, LocalSignerMetadataCapability,
        RustPublicFeedCapability,
    };

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionSide")]
    enum WirePositionSide {
        #[serde(rename = "long")]
        Long,
        #[serde(rename = "short")]
        Short,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionSide {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WirePositionSide::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionMode")]
    enum WirePositionMode {
        #[serde(rename = "hedged")]
        Hedged,
        #[serde(rename = "oneway")]
        OneWay,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionMode {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WirePositionMode::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarginType")]
    enum WireMarginType {
        #[serde(rename = "isolated")]
        Isolated,
        #[serde(rename = "cross")]
        Cross,
    }

    impl<'de> serde::Deserialize<'de> for super::MarginType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireMarginType::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketFees")]
    struct WireMarketFees {
        maker: f64,
        taker: f64,
    }
    impl<'de> serde::Deserialize<'de> for super::MarketFees {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketFees::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketRuntime")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketRuntime {
        #[serde(
            rename = "type",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        kind: Option<String>,
        #[serde(
            rename = "spot",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        spot: Option<bool>,
        #[serde(
            rename = "swap",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        swap: Option<bool>,
        #[serde(
            rename = "future",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        future: Option<bool>,
        #[serde(
            rename = "contract",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        contract: Option<bool>,
        #[serde(
            rename = "linear",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        linear: Option<bool>,
        #[serde(
            rename = "inverse",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        inverse: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fees: Option<super::MarketFees>,
    }
    impl<'de> serde::Deserialize<'de> for super::MarketRuntime {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketRuntime::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketPrecision")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketPrecision {
        amount: f64,
        price: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        base: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        quote: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::MarketPrecision {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketPrecision::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketLimitsAmount")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketLimitsAmount {
        min: f64,
        max: super::NumericValue,
    }

    impl<'de> serde::Deserialize<'de> for super::MarketLimitsAmount {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketLimitsAmount::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketLimitsLeverage")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketLimitsLeverage {
        min: f64,
        max: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::MarketLimitsLeverage {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketLimitsLeverage::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketLimits")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketLimits {
        amount: MarketLimitsAmount,
        leverage: MarketLimitsLeverage,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<MarketLimitsPrice>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        cost: Option<MarketLimitsLeverage>,
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MarketLimitsPrice")]
    #[serde(rename_all = "camelCase")]
    struct WireMarketLimitsPrice {
        min: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        max: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::MarketLimitsPrice {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketLimitsPrice::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionRight")]
    enum WireOptionRight {
        #[serde(rename = "call")]
        Call,
        #[serde(rename = "put")]
        Put,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionRight {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOptionRight::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionTermsStyle")]
    enum WireOptionTermsStyle {
        #[serde(rename = "european")]
        European,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionTermsStyle {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOptionTermsStyle::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionTermsSettlement")]
    enum WireOptionTermsSettlement {
        #[serde(rename = "cash")]
        Cash,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionTermsSettlement {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOptionTermsSettlement::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionTerms")]
    #[serde(rename_all = "camelCase")]
    struct WireOptionTerms {
        underlying: String,
        expiry: f64,
        strike: f64,
        right: OptionRight,
        style: OptionTermsStyle,
        settlement: OptionTermsSettlement,
        premium_asset: String,
        settle_asset: String,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionTerms {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOptionTerms::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionGreeks")]
    #[serde(rename_all = "camelCase")]
    struct WireOptionGreeks {
        delta: f64,
        gamma: f64,
        vega: f64,
        theta: f64,
        units: String,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionGreeks {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOptionGreeks::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    impl<'de> serde::Deserialize<'de> for super::MarketLimits {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireMarketLimits::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Market")]
    #[serde(rename_all = "camelCase")]
    struct WireMarket {
        id: String,
        symbol: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        pretty_symbol: Option<String>,
        base: String,
        quote: String,
        active: bool,
        precision: MarketPrecision,
        limits: MarketLimits,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        option: Option<OptionTerms>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        contract_size: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_expiring: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_inverse: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_spot: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_perpetual: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_read_only: Option<bool>,
        #[serde(
            default,
            deserialize_with = "crate::retained_json::deserialize_present_retained_info",
            skip_serializing_if = "Option::is_none"
        )]
        info: Option<crate::retained_json::RetainedJsonInput>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        taker_fee: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        maker_fee: Option<f64>,
        #[serde(flatten)]
        runtime: super::MarketRuntime,
    }

    impl<'de> serde::Deserialize<'de> for super::Market {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct InfoVisitor;
            impl<'de> serde::de::Visitor<'de> for InfoVisitor {
                type Value = super::Market;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a Market record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    d: D,
                ) -> Result<Self::Value, D::Error> {
                    WireMarket::deserialize(crate::strict_decode::object(d))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::INFO_RECORD, InfoVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionQuote")]
    #[serde(rename_all = "camelCase")]
    struct WireOptionQuote {
        symbol: String,
        bid: f64,
        ask: f64,
        mark: f64,
        mark_iv: f64,
        bid_iv: f64,
        ask_iv: f64,
        underlying_price: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        forward_price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        greeks: Option<super::OptionGreeks>,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionQuote {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOptionQuote::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionChainQuery")]
    #[serde(rename_all = "camelCase")]
    struct WireOptionChainQuery {
        underlying: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        expiry: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        right: Option<super::OptionRight>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        strike_min: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        strike_max: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionChainQuery {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOptionChainQuery::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OptionChainRow")]
    #[serde(rename_all = "camelCase")]
    struct WireOptionChainRow {
        market: Market,
        quote: super::OptionQuote,
    }

    impl<'de> serde::Deserialize<'de> for super::OptionChainRow {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOptionChainRow::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::AccountGreeks")]
    #[serde(rename_all = "camelCase")]
    struct WireAccountGreeks {
        underlying: String,
        delta: f64,
        gamma: f64,
        vega: f64,
        theta: f64,
        units: String,
    }

    impl<'de> serde::Deserialize<'de> for super::AccountGreeks {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireAccountGreeks::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::HistoricalTradeOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireHistoricalTradeOptions {
        symbol: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        to: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from_id: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        account_name: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::HistoricalTradeOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireHistoricalTradeOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OHLCVOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireOHLCVOptions {
        symbol: String,
        interval: AnyTimeframe,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        to: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OHLCVOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOHLCVOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OHLCVSOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireOHLCVSOptions {
        symbols: Vec<String>,
        interval: Timeframe,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        to: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OHLCVSOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOHLCVSOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExecutionsOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireExecutionsOptions {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        symbol: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        to: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::ExecutionsOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExecutionsOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderHistoryOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderHistoryOptions {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        symbol: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        from: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        to: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        status: Option<Vec<OrderStatus>>,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderHistoryOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderHistoryOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::IncomeHistoryOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireIncomeHistoryOptions {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        symbol: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        income_type: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        start_time: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        end_time: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        limit: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::IncomeHistoryOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireIncomeHistoryOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::BothPositionSide")]
    enum WireBothPositionSide {
        #[serde(rename = "both")]
        Both,
    }

    impl<'de> serde::Deserialize<'de> for super::BothPositionSide {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireBothPositionSide::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Timeframe")]
    enum WireTimeframe {
        #[serde(rename = "1m")]
        M1,
        #[serde(rename = "3m")]
        M3,
        #[serde(rename = "5m")]
        M5,
        #[serde(rename = "15m")]
        M15,
        #[serde(rename = "30m")]
        M30,
        #[serde(rename = "1h")]
        H1,
        #[serde(rename = "2h")]
        H2,
        #[serde(rename = "4h")]
        H4,
        #[serde(rename = "6h")]
        H6,
        #[serde(rename = "12h")]
        H12,
        #[serde(rename = "1d")]
        D1,
        #[serde(rename = "1w")]
        W1,
    }

    impl<'de> serde::Deserialize<'de> for super::Timeframe {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireTimeframe::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExchangeName")]
    enum WireExchangeName {
        #[serde(rename = "apexomni")]
        Apexomni,
        #[serde(rename = "asterdex")]
        Asterdex,
        #[serde(rename = "backpack")]
        Backpack,
        #[serde(rename = "binance")]
        Binance,
        #[serde(rename = "binanceinverse")]
        Binanceinverse,
        #[serde(rename = "binanceoptions")]
        Binanceoptions,
        #[serde(rename = "binancespot")]
        Binancespot,
        #[serde(rename = "binancepm")]
        Binancepm,
        #[serde(rename = "bingx")]
        Bingx,
        #[serde(rename = "bingxspot")]
        Bingxspot,
        #[serde(rename = "bitget")]
        Bitget,
        #[serde(rename = "bitgetinverse")]
        Bitgetinverse,
        #[serde(rename = "bitgetusdc")]
        Bitgetusdc,
        #[serde(rename = "bitrue")]
        Bitrue,
        #[serde(rename = "bitunix")]
        Bitunix,
        #[serde(rename = "blofin")]
        Blofin,
        #[serde(rename = "bybit")]
        Bybit,
        #[serde(rename = "bybitdemo")]
        Bybitdemo,
        #[serde(rename = "bybitinverse")]
        Bybitinverse,
        #[serde(rename = "bybitspot")]
        Bybitspot,
        #[serde(rename = "hyrotrader")]
        Hyrotrader,
        #[serde(rename = "coinbase")]
        Coinbase,
        #[serde(rename = "coinbasespot")]
        Coinbasespot,
        #[serde(rename = "cryptocom")]
        Cryptocom,
        #[serde(rename = "grvt")]
        Grvt,
        #[serde(rename = "hyperliquid")]
        Hyperliquid,
        #[serde(rename = "hyperliquidspot")]
        Hyperliquidspot,
        #[serde(rename = "lighter")]
        Lighter,
        #[serde(rename = "lighter-robinhood")]
        LighterRobinhood,
        #[serde(rename = "mexc")]
        Mexc,
        #[serde(rename = "okx")]
        Okx,
        #[serde(rename = "okxdemo")]
        Okxdemo,
        #[serde(rename = "okxeu")]
        Okxeu,
        #[serde(rename = "okxeudemo")]
        Okxeudemo,
        #[serde(rename = "paradex")]
        Paradex,
        #[serde(rename = "phemex")]
        Phemex,
        #[serde(rename = "polymarket")]
        Polymarket,
        #[serde(rename = "kodiakfi")]
        Kodiakfi,
        #[serde(rename = "mode")]
        Mode,
        #[serde(rename = "txflow")]
        Txflow,
        #[serde(rename = "what")]
        What,
        #[serde(rename = "weex")]
        Weex,
        #[serde(rename = "woo")]
        Woo,
        #[serde(rename = "woofi")]
        Woofi,
        #[serde(rename = "kucoin")]
        Kucoin,
        #[serde(rename = "kucoinspot")]
        Kucoinspot,
        #[serde(rename = "kraken")]
        Kraken,
        #[serde(rename = "krakenspot")]
        Krakenspot,
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeName {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExchangeName::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ValidateAccountResult")]
    struct WireValidateAccountResult {
        #[serde(rename = "error")]
        error: AccountValidationError,
        #[serde(rename = "balance")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        balance: Option<Balance>,
    }

    impl<'de> serde::Deserialize<'de> for super::ValidateAccountResult {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireValidateAccountResult::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::RequestOutcome")]
    enum WireRequestOutcome {
        #[serde(rename = "ok")]
        Ok,
        #[serde(rename = "rateLimit")]
        RateLimit,
        #[serde(rename = "auth")]
        Auth,
        #[serde(rename = "network")]
        Network,
        #[serde(rename = "other")]
        Other,
    }

    impl<'de> serde::Deserialize<'de> for super::RequestOutcome {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireRequestOutcome::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::RequestOutcomeCounts")]
    struct WireRequestOutcomeCounts {
        ok: f64,
        #[serde(rename = "rateLimit")]
        rate_limit: f64,
        auth: f64,
        network: f64,
        other: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::RequestOutcomeCounts {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireRequestOutcomeCounts::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::MessageHandlerStatsType")]
    enum WireMessageHandlerStatsType {
        #[serde(rename = "public")]
        Public,
        #[serde(rename = "private")]
        Private,
        #[serde(rename = "futures")]
        Futures,
        #[serde(rename = "business")]
        Business,
    }

    impl<'de> serde::Deserialize<'de> for super::MessageHandlerStatsType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireMessageHandlerStatsType::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::SecondsTimeframe")]
    enum WireSecondsTimeframe {
        #[serde(rename = "1s")]
        S1,
        #[serde(rename = "5s")]
        S5,
        #[serde(rename = "15s")]
        S15,
        #[serde(rename = "30s")]
        S30,
    }

    impl<'de> serde::Deserialize<'de> for super::SecondsTimeframe {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireSecondsTimeframe::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ExtendedTimeframe")]
    enum WireExtendedTimeframe {
        #[serde(rename = "3h")]
        H3,
        #[serde(rename = "5h")]
        H5,
        #[serde(rename = "7h")]
        H7,
        #[serde(rename = "8h")]
        H8,
        #[serde(rename = "9h")]
        H9,
        #[serde(rename = "10h")]
        H10,
        #[serde(rename = "11h")]
        H11,
        #[serde(rename = "24h")]
        H24,
        #[serde(rename = "2d")]
        D2,
        #[serde(rename = "3d")]
        D3,
        #[serde(rename = "1M")]
        MN1,
    }

    impl<'de> serde::Deserialize<'de> for super::ExtendedTimeframe {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireExtendedTimeframe::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PublicExchangeOptions")]
    struct WirePublicExchangeOptions {
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
        #[serde(rename = "key")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        key: Option<String>,
        #[serde(rename = "secret")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        secret: Option<String>,
        #[serde(rename = "publicDataOnly")]
        public_data_only: True,
        #[serde(rename = "forceAllProxiesOff")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        force_all_proxies_off: Option<bool>,
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
        #[serde(
            rename = "polymarket",
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        polymarket: Option<ExchangeOptionsPolymarket>,
    }

    impl<'de> serde::Deserialize<'de> for super::PublicExchangeOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePublicExchangeOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderRiskProfile")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderRiskProfile {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_looking_like_a_full_stop: Option<bool>,
        order_pnl: f64,
        total_realized_pnl: f64,
        rolling_unrealized_pnl: f64,
        #[serde(default)]
        order_line_pos_side: ModelField<PositionSide>,
        position_size: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_line_text: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fee: Option<f64>,
        #[serde(skip)]
        position_size_before_fee: bool,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderRiskProfile {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;
            impl<'de> serde::de::Visitor<'de> for RecordVisitor {
                type Value = super::OrderRiskProfile;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a model record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    decoder: D,
                ) -> Result<Self::Value, D::Error> {
                    WireOrderRiskProfile::deserialize(crate::strict_decode::object(decoder))
                }
                // Map-forwarding decoders (flatten/MapAccessDeserializer) skip the hint.
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    map: A,
                ) -> Result<Self::Value, A::Error> {
                    WireOrderRiskProfile::deserialize(crate::strict_decode::object(
                        serde::de::value::MapAccessDeserializer::new(map),
                    ))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, RecordVisitor)
        }
    }

    #[cfg(test)]
    mod shape_tests {
        use super::*;
        #[test]
        fn every_shared_codec_has_a_narrow_input_shape() {
            crate::strict_decode::tests::string_union::<OptionRight>(
                "OptionRight",
                WireOptionRight::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                [OptionRight::Call, OptionRight::Put]
                    .into_iter()
                    .map(|right| {
                        serde_json::to_value(right)
                            .unwrap()
                            .as_str()
                            .unwrap()
                            .to_owned()
                    })
                    .collect(),
            );

            crate::strict_decode::tests::string_union::<PositionSide>(
                "PositionSide",
                WirePositionSide::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(PositionSide::Long)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(PositionSide::Short)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<PositionMode>(
                "PositionMode",
                WirePositionMode::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(PositionMode::Hedged)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(PositionMode::OneWay)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<MarginType>(
                "MarginType",
                WireMarginType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(MarginType::Isolated)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(MarginType::Cross)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<MarketPrecision>("MarketPrecision");
            crate::strict_decode::tests::record::<MarketLimitsAmount>("MarketLimitsAmount");
            crate::strict_decode::tests::record::<MarketLimitsLeverage>("MarketLimitsLeverage");
            crate::strict_decode::tests::record::<MarketLimits>("MarketLimits");
            crate::strict_decode::tests::record::<Market>("Market");

            crate::strict_decode::tests::record::<CoinBalance>("CoinBalance");
            crate::strict_decode::tests::record::<Balance>("Balance");
            crate::strict_decode::tests::record::<Ticker>("Ticker");
            crate::strict_decode::tests::record::<PublicTrade>("PublicTrade");
            crate::strict_decode::tests::record::<OptionTerms>("OptionTerms");
            crate::strict_decode::tests::record::<OptionGreeks>("OptionGreeks");
            crate::strict_decode::tests::record::<OptionQuote>("OptionQuote");
            crate::strict_decode::tests::record::<OptionChainQuery>("OptionChainQuery");
            crate::strict_decode::tests::record::<OptionChainRow>("OptionChainRow");
            crate::strict_decode::tests::record::<AccountGreeks>("AccountGreeks");
            crate::strict_decode::tests::record::<Candle>("Candle");
            crate::strict_decode::tests::record::<Execution>("Execution");
            crate::strict_decode::tests::record::<OrderFillEvent>("OrderFillEvent");
            crate::strict_decode::tests::record::<OrderBookOrders>("OrderBookOrders");
            crate::strict_decode::tests::record::<OrderBook>("OrderBook");

            crate::strict_decode::tests::record::<HistoricalTrade>("HistoricalTrade");
            crate::strict_decode::tests::record::<HistoricalTradeOptions>("HistoricalTradeOptions");
            crate::strict_decode::tests::record::<OHLCVOptions>("OHLCVOptions");
            crate::strict_decode::tests::record::<OHLCVSOptions>("OHLCVSOptions");
            crate::strict_decode::tests::record::<ExecutionsOptions>("ExecutionsOptions");
            crate::strict_decode::tests::record::<OrderHistoryOptions>("OrderHistoryOptions");
            crate::strict_decode::tests::record::<HistoricalOrder>("HistoricalOrder");
            crate::strict_decode::tests::record::<IncomeHistoryOptions>("IncomeHistoryOptions");
            crate::strict_decode::tests::string_union::<BothPositionSide>(
                "BothPositionSide",
                WireBothPositionSide::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(BothPositionSide::Both)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<Timeframe>(
                "Timeframe",
                WireTimeframe::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(Timeframe::M1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::M3)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::M5)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::M15)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::M30)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::H1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::H2)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::H4)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::H6)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::H12)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::D1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(Timeframe::W1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<ExchangeName>(
                "ExchangeName",
                WireExchangeName::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(ExchangeName::Apexomni)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Asterdex)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Backpack)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Binance)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Binanceinverse)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Binanceoptions)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Binancespot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Binancepm)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bingx)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bingxspot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bitget)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bitgetinverse)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bitgetusdc)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bitrue)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bitunix)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Blofin)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bybit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bybitdemo)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bybitinverse)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Bybitspot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Hyrotrader)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Coinbase)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Coinbasespot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Cryptocom)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Grvt)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Hyperliquid)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Hyperliquidspot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Lighter)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::LighterRobinhood)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Mexc)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Okx)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Okxdemo)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Okxeu)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Okxeudemo)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Paradex)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Phemex)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Polymarket)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Kodiakfi)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Mode)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Txflow)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::What)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Weex)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Woo)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Woofi)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Kucoin)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Kucoinspot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Kraken)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::Krakenspot)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<ValidateAccountResult>("ValidateAccountResult");

            crate::strict_decode::tests::record::<ClosedPnlEntry>("ClosedPnlEntry");

            crate::strict_decode::tests::record::<IncomeRecord>("IncomeRecord");
            crate::strict_decode::tests::string_union::<RequestOutcome>(
                "RequestOutcome",
                WireRequestOutcome::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(RequestOutcome::Ok)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(RequestOutcome::RateLimit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(RequestOutcome::Auth)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(RequestOutcome::Network)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(RequestOutcome::Other)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<RequestOutcomeCounts>("RequestOutcomeCounts");
            crate::strict_decode::tests::string_union::<MessageHandlerStatsType>(
                "MessageHandlerStatsType",
                WireMessageHandlerStatsType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(MessageHandlerStatsType::Public)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(MessageHandlerStatsType::Private)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(MessageHandlerStatsType::Futures)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(MessageHandlerStatsType::Business)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<SecondsTimeframe>(
                "SecondsTimeframe",
                WireSecondsTimeframe::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(SecondsTimeframe::S1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(SecondsTimeframe::S5)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(SecondsTimeframe::S15)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(SecondsTimeframe::S30)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<ExtendedTimeframe>(
                "ExtendedTimeframe",
                WireExtendedTimeframe::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(ExtendedTimeframe::H3)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H5)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H7)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H8)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H9)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H10)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H11)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::H24)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::D2)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::D3)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExtendedTimeframe::MN1)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<PublicExchangeOptions>("PublicExchangeOptions");
            crate::strict_decode::tests::record::<OrderRiskProfile>("OrderRiskProfile");
        }
    }
}

/// Canonical remote-signer WS request and response payloads.
pub mod ws_auth;

/// Canonical linear-perpetual journal contracts.
pub mod position_record;
pub use position_record::{
    PositionCloseReason, PositionProvenance, PositionRecord, PositionRecordLeg,
    PositionRecordState, PositionSourceAxis,
};

/// Host callback failures shared by Store and its public-data view.
pub mod subscriber_error;
pub use subscriber_error::{PublicDataSubscriber, StoreSubscriber, StoreSubscriberError};

pub mod exchange_metadata;

pub mod prepared_order;
pub use prepared_order::PreparedOrderOpts;
