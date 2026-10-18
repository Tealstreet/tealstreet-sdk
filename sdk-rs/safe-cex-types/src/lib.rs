//! Rust-owned finite JSON domain models. See ../TYPE_PARITY.md for generator compatibility limits.
use serde::{Deserialize, Serialize};
use ts_rs::TS;

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

/// Typed account extension consumed by the Polymarket source constructor.
/// Unlike the venue's retained UTF-16 record, this uses the shared account
/// UTF-8 string boundary. signerType retains source numeric values verbatim.
#[derive(Debug, Clone, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ExchangeOptionsPolymarket {
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
    pub signer_type: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub funder_address: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderStatus {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "closed")]
    Closed,
    #[serde(rename = "canceled")]
    Canceled,
    #[serde(rename = "creating")]
    Creating,
    #[serde(rename = "updating")]
    Updating,
    #[serde(rename = "canceling")]
    Canceling,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderType {
    #[serde(rename = "market")]
    Market,
    #[serde(rename = "limit")]
    Limit,
    #[serde(rename = "conditional")]
    Conditional,
    #[serde(rename = "stop_market")]
    StopLoss,
    #[serde(rename = "take_profit_market")]
    TakeProfit,
    #[serde(rename = "trailing_stop_market")]
    TrailingStopLoss,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderSide {
    #[serde(rename = "buy")]
    Buy,
    #[serde(rename = "sell")]
    Sell,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderTimeInForce {
    #[serde(rename = "GoodTillCancel")]
    GoodTillCancel,
    #[serde(rename = "ImmediateOrCancel")]
    ImmediateOrCancel,
    #[serde(rename = "FillOrKill")]
    FillOrKill,
    #[serde(rename = "PostOnly")]
    PostOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderStopTriggerType {
    #[serde(rename = "mark_price")]
    MarkPrice,
    #[serde(rename = "last_price")]
    LastPrice,
    #[serde(rename = "index_price")]
    IndexPrice,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, TS)]
pub enum OrderRejectReason {
    #[serde(rename = "PostOnly")]
    PostOnly,
    #[serde(rename = "InsufficientBalance")]
    InsufficientBalance,
    #[serde(rename = "ReduceOnly")]
    ReduceOnly,
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

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub symbol: String,

    pub pretty_symbol: Option<String>,
    pub base: String,
    pub quote: String,
    pub active: bool,
    #[ts(inline)]
    pub precision: MarketPrecision,
    #[ts(inline)]
    pub limits: MarketLimits,

    pub option: Option<OptionTerms>,

    pub contract_size: Option<f64>,

    pub is_expiring: Option<bool>,

    pub is_inverse: Option<bool>,

    pub is_spot: Option<bool>,

    pub is_perpetual: Option<bool>,

    pub is_read_only: Option<bool>,

    #[ts(type = "any", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,

    pub taker_fee: Option<f64>,

    pub maker_fee: Option<f64>,
    /// Runtime venue properties; serialized at the Market object's top level.
    #[ts(skip)]
    pub runtime: MarketRuntime,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderExtra {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub remaining_native: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub filled_in_placement_units: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub remaining_notional: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub conditional_order_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reject_reason: Option<OrderRejectReason>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_from_chaser: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_attempts: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub chaser_max_attempts: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Order {
    pub id: String,
    #[ts(as = "Option<String>")]
    pub parent_id: ModelField<String>,
    #[ts(as = "OrderStatus")]
    pub status: ModelField<OrderStatus>,
    pub symbol: String,
    #[ts(as = "OrderType")]
    pub r#type: ModelField<OrderType>,
    #[ts(as = "OrderSide")]
    pub side: ModelField<OrderSide>,
    #[ts(as = "f64")]
    pub price: ModelField<f64>,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    #[ts(as = "bool")]
    pub reduce_only: ModelField<bool>,
    #[ts(as = "Option<bool>")]
    pub close: ModelField<bool>,
    #[ts(type = "any", optional)]
    pub info: ModelField<crate::retained_json::RetainedJsonInput>,
    // The venue client order ID where the adapter maps it to the top level
    // (Hyperliquid's cloid): `info` is stripped before a worker posts the store.
    #[ts(as = "Option<String>")]
    pub client_order_id: ModelField<String>,
    #[ts(as = "Option<OrderTimeInForce>")]
    pub time_in_force: ModelField<OrderTimeInForce>,
    pub last_update: f64,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub tp_trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<OrderStopTriggerType>")]
    pub sl_trigger_type: ModelField<OrderStopTriggerType>,
    #[ts(as = "Option<f64>")]
    pub take_profit: ModelField<f64>,
    #[ts(as = "Option<f64>")]
    pub stop_loss: ModelField<f64>,
    #[ts(inline)]
    #[ts(as = "Option<OrderExtra>")]
    pub extra: ModelField<OrderExtra>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PositionOption {
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
pub struct PositionExtra {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub contracts_native: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub unrealized_pnl_native: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Position {
    pub id: String,
    pub symbol: String,
    #[ts(as = "PositionSide")]
    pub side: ModelField<PositionSide>,
    pub entry_price: f64,
    pub notional: f64,
    pub leverage: f64,
    #[ts(as = "Option<MarginType>")]
    pub margin_type: ModelField<MarginType>,
    #[ts(as = "Option<PositionMode>")]
    pub position_mode: ModelField<PositionMode>,
    pub unrealized_pnl: f64,
    #[ts(as = "Option<f64>")]
    pub realized_pnl: ModelField<f64>,
    pub contracts: f64,
    pub liquidation_price: f64,
    #[ts(as = "Option<f64>")]
    pub margin_size: ModelField<f64>,
    #[ts(type = "any", optional)]
    pub info: ModelField<crate::retained_json::RetainedJsonInput>,
    #[ts(inline)]
    #[ts(as = "Option<PositionExtra>")]
    pub extra: ModelField<PositionExtra>,
    #[ts(as = "Option<f64>")]
    pub last_update: ModelField<f64>,
    #[ts(inline)]
    #[ts(as = "Option<PositionOption>")]
    pub option: ModelField<PositionOption>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct CoinBalance {
    /// @ts-mutable
    pub base: String,
    /// @ts-mutable
    pub amount: f64,
    /// @ts-mutable
    pub notional: f64,
}

/// A currency row preserves the shape returned by its source venue. Crypto.com
/// REST returns available/locked rather than the statically declared Balance.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(untagged)]
pub enum CurrencyBalance {
    Declared(Balance),
    AvailableLocked { available: f64, locked: f64 },
}

impl CurrencyBalance {
    /// Read a declared Balance row without normalizing other source shapes.
    pub fn as_balance(&self) -> Option<&Balance> {
        match self {
            Self::Declared(balance) => Some(balance),
            Self::AvailableLocked { .. } => None,
        }
    }
}
impl From<Balance> for CurrencyBalance {
    fn from(balance: Balance) -> Self {
        Self::Declared(balance)
    }
}
#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Balance {
    pub used: f64,
    pub free: f64,
    pub total: f64,
    pub upnl: f64,

    #[ts(type = "any", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,

    #[ts(type = "{ [currency: string]: Balance }", optional)]
    pub currencies: Option<crate::object_map::JsObjectMap<crate::CurrencyBalance>>,

    #[ts(type = "{ [coin: string]: CoinBalance }", optional)]
    pub coins: Option<crate::object_map::JsObjectMap<CoinBalance>>,
}

#[derive(Debug, Clone, PartialEq, TS)]
#[ts(optional_fields, rename_all = "camelCase")]
pub struct Ticker {
    pub id: String,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub mark: f64,
    // WOO publishes null for a real market with no index price.
    #[ts(as = "f64")]
    pub index: Option<f64>,

    pub mid_price: Option<f64>,
    pub percentage: f64,
    pub open_interest: f64,
    pub funding_rate: f64,
    pub funding_time: f64,
    pub volume: f64,
    pub quote_volume: f64,
    // Runtime extra emitted by Paradex fetchTickers from virtualClock.getCurrentTimeMs().
    // The generated TypeScript remains the source declaration projection.
    #[ts(skip)]
    pub timestamp: Option<f64>,

    #[ts(type = "any", optional)]
    pub info: Option<crate::retained_json::RetainedJsonInput>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PublicTrade {
    pub id: String,
    pub symbol: String,
    pub price: f64,
    pub qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quote_qty: Option<f64>,
    pub timestamp: f64,
    pub side: OrderSide,
}

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
pub struct Candle {
    pub timestamp: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub last_side: Option<OrderSide>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct Execution {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    pub timestamp: f64,
    pub symbol: String,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub order_id: Option<String>,
    pub price: f64,
    pub qty: f64,
    pub side: OrderSide,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission_asset: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub position_side: Option<ExecutionPositionSide>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub venue_reported_revenue: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub venue_reported_revenue_asset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderBookOrders {
    /// @ts-mutable
    pub price: f64,
    /// @ts-mutable
    pub amount: f64,
    /// @ts-mutable
    pub total: f64,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct OrderBook {
    /// @ts-mutable
    pub bids: Vec<OrderBookOrders>,
    /// @ts-mutable
    pub asks: Vec<OrderBookOrders>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct PlaceOrderOpts {
    pub symbol: String,
    pub r#type: OrderType,
    pub side: OrderSide,
    pub amount: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub stop_loss: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub take_profit: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reduce_only: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub time_in_force: Option<OrderTimeInForce>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub tp_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub sl_trigger_type: Option<OrderStopTriggerType>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_order_id: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub max_slippage_pct: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub close: Option<bool>,
    /// Price an option order by implied volatility, as a fraction. Option markets only.
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub implied_volatility: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct HistoricalTrade {
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub id: Option<String>,
    pub symbol: String,
    pub order_id: String,
    pub price: f64,
    pub qty: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub quote_qty: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    #[ts(inline)]
    pub position_side: ExecutionPositionSide,
    pub side: OrderSide,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub buyer: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub maker: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub commission_asset: Option<String>,
    pub time: f64,
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
pub struct HistoricalOrder {
    pub id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub r#type: OrderType,
    pub status: OrderStatus,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub price: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub average_price: Option<f64>,
    pub amount: f64,
    pub filled: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub reduce_only: Option<bool>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(inline)]
    pub position_side: Option<ExecutionPositionSide>,
    pub created_time: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub updated_time: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub realized_pnl: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fees: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub fee_asset: Option<String>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rebate: Option<f64>,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub rebate_asset: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct StoreDataLoaded {
    pub balance: bool,
    pub orders: bool,
    pub markets: bool,
    pub tickers: bool,
    pub positions: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct StoreOptions {
    pub is_hedged: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct FetchedLeverage {
    /// @ts-mutable
    pub buy_leverage: f64,
    /// @ts-mutable
    pub sell_leverage: f64,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub margin_type: Option<MarginType>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub position_mode: Option<PositionMode>,
    /// @ts-mutable
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub is_fetching: Option<bool>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
#[ts(optional_fields)]
pub struct DisplayedMargin {
    pub long_exposure: f64,
    pub short_exposure: f64,
    pub total_notional: f64,
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
    #[serde(rename = "binancespot")]
    Binancespot,
    #[serde(rename = "binancepm")]
    Binancepm,
    #[serde(rename = "bingx")]
    Bingx,
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
    #[serde(rename = "breakoutprop")]
    Breakoutprop,
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
    #[serde(rename = "salsadex")]
    Salsadex,
    #[serde(rename = "tealstreet-dex")]
    TealstreetDex,
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ExchangeOptions {
    #[serde(rename = "key")]
    pub key: String,
    #[serde(rename = "secret")]
    pub secret: String,
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
    /// Runtime extension read by Polymarket through its source `opts as any`.
    #[serde(rename = "polymarket")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub polymarket: Option<crate::ExchangeOptionsPolymarket>,
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

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOptsUpdateVariant0 {
    #[serde(rename = "amount")]
    pub amount: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub price: Option<f64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOptsUpdateVariant1 {
    #[serde(rename = "price")]
    pub price: f64,
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    #[ts(skip)]
    pub amount: Option<f64>,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum UpdateOrderOptsUpdate {
    Variant0(#[ts(inline)] UpdateOrderOptsUpdateVariant0),
    Variant1(#[ts(inline)] UpdateOrderOptsUpdateVariant1),
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct UpdateOrderOpts {
    #[serde(rename = "order")]
    pub order: Order,
    #[serde(rename = "update")]
    #[ts(inline)]
    pub update: UpdateOrderOptsUpdate,
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
pub enum DataProvenance {
    #[serde(rename = "venue")]
    Venue,
    #[serde(rename = "derived")]
    Derived,
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryNotional {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryEntryPrice {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryExitPrice {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryUnrealizedPnl {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryFees {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryContracts {
    Variant0(String),
    Variant1(f64),
}

/// @ts-shape
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, TS)]
#[serde(untagged)]
pub enum ClosedPnlEntryLeverage {
    Variant0(f64),
    Variant1(String),
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct ClosedPnlEntry {
    /// @ts-mutable
    #[serde(rename = "provenance")]
    pub provenance: DataProvenance,
    /// @ts-mutable
    #[serde(rename = "symbol")]
    pub symbol: String,
    /// @ts-mutable
    #[serde(rename = "notional")]
    #[ts(inline)]
    pub notional: ClosedPnlEntryNotional,
    /// @ts-mutable
    #[serde(rename = "entryPrice")]
    #[ts(inline)]
    pub entry_price: ClosedPnlEntryEntryPrice,
    /// @ts-mutable
    #[serde(rename = "exitPrice")]
    #[ts(inline)]
    pub exit_price: ClosedPnlEntryExitPrice,
    /// @ts-mutable
    #[serde(rename = "unrealizedPnl")]
    #[ts(inline)]
    pub unrealized_pnl: ClosedPnlEntryUnrealizedPnl,
    /// @ts-mutable
    #[serde(rename = "side")]
    pub side: String,
    /// @ts-mutable
    #[serde(rename = "fees")]
    #[ts(inline)]
    pub fees: ClosedPnlEntryFees,
    /// @ts-mutable
    #[serde(rename = "time")]
    pub time: f64,
    /// @ts-mutable
    #[serde(rename = "closedTime")]
    pub closed_time: f64,
    /// @ts-mutable
    #[serde(rename = "account")]
    pub account: String,
    /// @ts-mutable
    #[serde(rename = "contracts")]
    #[ts(inline)]
    pub contracts: ClosedPnlEntryContracts,
    /// @ts-mutable
    #[serde(rename = "leverage")]
    #[ts(inline)]
    pub leverage: ClosedPnlEntryLeverage,
}

/// @ts-union
#[derive(Debug, Clone, PartialEq, Serialize, TS)]
pub enum IncomeCategory {
    #[serde(rename = "realized_pnl")]
    RealizedPnl,
    #[serde(rename = "funding")]
    Funding,
    #[serde(rename = "commission")]
    Commission,
    #[serde(rename = "rebate")]
    Rebate,
    #[serde(rename = "transfer")]
    Transfer,
    #[serde(rename = "liquidation")]
    Liquidation,
    #[serde(rename = "other")]
    Other,
}

#[derive(Debug, Clone, PartialEq, Serialize, TS)]
#[ts(optional_fields)]
pub struct IncomeRecord {
    /// @ts-mutable
    #[serde(rename = "symbol")]
    pub symbol: String,
    /// @ts-mutable
    #[serde(rename = "incomeType")]
    pub income_type: String,
    /// @ts-mutable
    #[serde(rename = "income")]
    pub income: f64,
    /// @ts-mutable
    #[serde(rename = "asset")]
    pub asset: String,
    /// @ts-mutable
    #[serde(rename = "timestamp")]
    pub timestamp: f64,
    /// @ts-mutable
    #[serde(rename = "tranId")]
    pub tran_id: String,
    /// @ts-mutable
    #[serde(rename = "tradeId")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub trade_id: Option<String>,
    /// @ts-mutable
    #[serde(rename = "category")]
    #[serde(
        default,
        deserialize_with = "deserialize_present",
        skip_serializing_if = "Option::is_none"
    )]
    pub category: Option<IncomeCategory>,
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
pub struct StoreUpdateIds {
    /// @ts-mutable
    #[serde(rename = "positions")]
    pub positions: f64,
    /// @ts-mutable
    #[serde(rename = "orders")]
    pub orders: f64,
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
pub use store_model::{ActiveChaser, ActiveTWAP, DisplayedPosition, StoreData};

pub mod extra_model;
pub mod model_utils;
pub mod symbol_converter;
pub use extra_model::{
    OrderFillEvent, PublicDataSource, PublicDataStore, READ_ONLY_ORDER_STATUSES,
    TIMEFRAME_TO_SECONDS, Writable, WritableStoreData,
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
    use super::{
        AccountGreeks, BothPositionSide, Candle, ClosedPnlEntry, DisplayedMargin, ExchangeAccount,
        ExchangeAccountConfig, ExchangeOptions, ExchangeOptionsExternalPublicDataVariant0,
        ExchangeOptionsExternalPublicDataVariant1, Execution, ExecutionsOptions, ExtendedTimeframe,
        FetchedLeverage, HistoricalOrder, HistoricalTrade, HistoricalTradeOptions,
        IncomeHistoryOptions, IncomeRecord, Market, MessageHandlerStatsType, OHLCVOptions,
        OHLCVSOptions, OptionChainQuery, OptionChainRow, OptionQuote, OrderBook,
        OrderHistoryOptions, OrderRiskProfile, PlaceOrderOpts, Position, PublicExchangeOptions,
        PublicTrade, RequestOutcome, RequestOutcomeCounts, SecondsTimeframe, StoreDataLoaded,
        StoreOptions, StoreUpdateIds, Ticker, UpdateOrderOpts, UpdateOrderOptsUpdateVariant0,
        UpdateOrderOptsUpdateVariant1, ValidateAccountResult,
    };
    use super::{
        AccountValidationError, AnyTimeframe, Balance, ClosedPnlEntryContracts,
        ClosedPnlEntryEntryPrice, ClosedPnlEntryExitPrice, ClosedPnlEntryFees,
        ClosedPnlEntryLeverage, ClosedPnlEntryNotional, ClosedPnlEntryUnrealizedPnl, CoinBalance,
        DataProvenance, ExchangeAccountConfigBybitSiteId, ExchangeAccountConfigHealthStatus,
        ExchangeName, ExchangeOptionsAuth, ExchangeOptionsBybitSiteId,
        ExchangeOptionsExternalPublicData, ExchangeOptionsExternalPublicDataVariant0Type,
        ExchangeOptionsExternalPublicDataVariant1Type, ExchangeOptionsExtra,
        ExchangeOptionsExtraAsterdexApiVersion, ExchangeOptionsExtraHyrotraderMode,
        ExchangeOptionsExtraKucoinBroker, ExchangeOptionsExtraLighterBuilderFeeMode,
        ExchangeOptionsHyperliquidBuilder, ExchangeOptionsTuning, ExecutionPositionSide,
        IncomeCategory, MarginType, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage,
        MarketLimitsPrice, MarketPrecision, ModelField, OptionGreeks, OptionRight, OptionTerms,
        OptionTermsSettlement, OptionTermsStyle, Order, OrderBookOrders, OrderExtra,
        OrderRejectReason, OrderSide, OrderStatus, OrderStopTriggerType, OrderTimeInForce,
        OrderType, PayloadSignerCapability, PositionExtra, PositionMode, PositionOption,
        PositionSide, PublicDataSourceCapability, RemoteSignerCapability, Timeframe,
        TransportCapability, True, UpdateOrderOptsUpdate, deserialize_present,
    };
    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderStatus")]
    enum WireOrderStatus {
        #[serde(rename = "open")]
        Open,
        #[serde(rename = "closed")]
        Closed,
        #[serde(rename = "canceled")]
        Canceled,
        #[serde(rename = "creating")]
        Creating,
        #[serde(rename = "updating")]
        Updating,
        #[serde(rename = "canceling")]
        Canceling,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderStatus {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderStatus::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderType")]
    enum WireOrderType {
        #[serde(rename = "market")]
        Market,
        #[serde(rename = "limit")]
        Limit,
        #[serde(rename = "conditional")]
        Conditional,
        #[serde(rename = "stop_market")]
        StopLoss,
        #[serde(rename = "take_profit_market")]
        TakeProfit,
        #[serde(rename = "trailing_stop_market")]
        TrailingStopLoss,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderType::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderSide")]
    enum WireOrderSide {
        #[serde(rename = "buy")]
        Buy,
        #[serde(rename = "sell")]
        Sell,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderSide {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderSide::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(value))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderTimeInForce")]
    enum WireOrderTimeInForce {
        #[serde(rename = "GoodTillCancel")]
        GoodTillCancel,
        #[serde(rename = "ImmediateOrCancel")]
        ImmediateOrCancel,
        #[serde(rename = "FillOrKill")]
        FillOrKill,
        #[serde(rename = "PostOnly")]
        PostOnly,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderTimeInForce {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderTimeInForce::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderStopTriggerType")]
    // Remote variants must retain the canonical public variant names.
    #[allow(clippy::enum_variant_names)]
    enum WireOrderStopTriggerType {
        #[serde(rename = "mark_price")]
        MarkPrice,
        #[serde(rename = "last_price")]
        LastPrice,
        #[serde(rename = "index_price")]
        IndexPrice,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderStopTriggerType {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderStopTriggerType::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderRejectReason")]
    enum WireOrderRejectReason {
        #[serde(rename = "PostOnly")]
        PostOnly,
        #[serde(rename = "InsufficientBalance")]
        InsufficientBalance,
        #[serde(rename = "ReduceOnly")]
        ReduceOnly,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderRejectReason {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireOrderRejectReason::deserialize(
                serde::de::value::StringDeserializer::<D::Error>::new(value),
            )
        }
    }

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

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionOption")]
    #[serde(rename_all = "camelCase")]
    struct WirePositionOption {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        greeks: Option<OptionGreeks>,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionOption {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionOption::deserialize(crate::strict_decode::object(deserializer))
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
    #[serde(remote = "super::OrderExtra")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderExtra {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        remaining_native: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        filled_in_placement_units: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        remaining_notional: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        conditional_order_price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        reject_reason: Option<OrderRejectReason>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_from_chaser: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_attempts: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        chaser_max_attempts: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderExtra {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderExtra::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Order")]
    #[serde(rename_all = "camelCase")]
    struct WireOrder {
        id: String,
        #[serde(default)]
        parent_id: ModelField<String>,
        #[serde(default)]
        status: ModelField<OrderStatus>,
        symbol: String,
        #[serde(default)]
        r#type: ModelField<OrderType>,
        #[serde(default)]
        side: ModelField<OrderSide>,
        #[serde(default)]
        price: ModelField<f64>,
        amount: f64,
        filled: f64,
        remaining: f64,
        #[serde(default)]
        reduce_only: ModelField<bool>,
        #[serde(default)]
        close: ModelField<bool>,
        #[serde(default, deserialize_with = "crate::model_field::deserialize_info")]
        info: ModelField<crate::retained_json::RetainedJsonInput>,
        #[serde(default)]
        client_order_id: ModelField<String>,
        #[serde(default)]
        time_in_force: ModelField<OrderTimeInForce>,
        last_update: f64,
        #[serde(default)]
        trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        tp_trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        sl_trigger_type: ModelField<OrderStopTriggerType>,
        #[serde(default)]
        take_profit: ModelField<f64>,
        #[serde(default)]
        stop_loss: ModelField<f64>,
        #[serde(default)]
        extra: ModelField<OrderExtra>,
    }

    impl<'de> serde::Deserialize<'de> for super::Order {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;
            impl<'de> serde::de::Visitor<'de> for RecordVisitor {
                type Value = super::Order;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a model record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    decoder: D,
                ) -> Result<Self::Value, D::Error> {
                    WireOrder::deserialize(crate::strict_decode::object(decoder))
                }
                // Map-forwarding decoders (flatten/MapAccessDeserializer) skip the hint.
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    map: A,
                ) -> Result<Self::Value, A::Error> {
                    WireOrder::deserialize(crate::strict_decode::object(
                        serde::de::value::MapAccessDeserializer::new(map),
                    ))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, RecordVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PositionExtra")]
    #[serde(rename_all = "camelCase")]
    struct WirePositionExtra {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        contracts_native: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        unrealized_pnl_native: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::PositionExtra {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePositionExtra::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Position")]
    #[serde(rename_all = "camelCase")]
    struct WirePosition {
        id: String,
        symbol: String,
        #[serde(default)]
        side: ModelField<PositionSide>,
        entry_price: f64,
        notional: f64,
        leverage: f64,
        #[serde(default)]
        margin_type: ModelField<MarginType>,
        #[serde(default)]
        position_mode: ModelField<PositionMode>,
        unrealized_pnl: f64,
        #[serde(default)]
        realized_pnl: ModelField<f64>,
        contracts: f64,
        liquidation_price: f64,
        #[serde(default)]
        margin_size: ModelField<f64>,
        #[serde(default, deserialize_with = "crate::model_field::deserialize_info")]
        info: ModelField<crate::retained_json::RetainedJsonInput>,
        #[serde(default)]
        extra: ModelField<PositionExtra>,
        #[serde(default)]
        last_update: ModelField<f64>,
        #[serde(default)]
        option: ModelField<PositionOption>,
    }

    impl<'de> serde::Deserialize<'de> for super::Position {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct RecordVisitor;
            impl<'de> serde::de::Visitor<'de> for RecordVisitor {
                type Value = super::Position;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a model record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    decoder: D,
                ) -> Result<Self::Value, D::Error> {
                    WirePosition::deserialize(crate::strict_decode::object(decoder))
                }
                // Map-forwarding decoders (flatten/MapAccessDeserializer) skip the hint.
                fn visit_map<A: serde::de::MapAccess<'de>>(
                    self,
                    map: A,
                ) -> Result<Self::Value, A::Error> {
                    WirePosition::deserialize(crate::strict_decode::object(
                        serde::de::value::MapAccessDeserializer::new(map),
                    ))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::MODEL_RECORD, RecordVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::CoinBalance")]
    #[serde(rename_all = "camelCase")]
    struct WireCoinBalance {
        base: String,
        amount: f64,
        notional: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::CoinBalance {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireCoinBalance::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    struct AvailableLockedBalance {
        available: f64,
        locked: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::CurrencyBalance {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let input = crate::retained_json::RetainedJsonInput::deserialize(deserializer)?;
            if !input.is_object() {
                return Err(serde::de::Error::custom(
                    "CurrencyBalance must be an object",
                ));
            }
            if let Ok(balance) = input.clone().decode_native::<super::Balance>(false) {
                return Ok(Self::Declared(balance));
            }
            let value = input
                .decode_native::<AvailableLockedBalance>(false)
                .map_err(serde::de::Error::custom)?;
            Ok(Self::AvailableLocked {
                available: value.available,
                locked: value.locked,
            })
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Balance")]
    #[serde(rename_all = "camelCase")]
    struct WireBalance {
        used: f64,
        free: f64,
        total: f64,
        upnl: f64,
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
        currencies: Option<crate::object_map::JsObjectMap<crate::CurrencyBalance>>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        coins: Option<crate::object_map::JsObjectMap<CoinBalance>>,
    }

    impl<'de> serde::Deserialize<'de> for super::Balance {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct InfoVisitor;
            impl<'de> serde::de::Visitor<'de> for InfoVisitor {
                type Value = super::Balance;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a Balance record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    d: D,
                ) -> Result<Self::Value, D::Error> {
                    WireBalance::deserialize(crate::strict_decode::object(d))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::INFO_RECORD, InfoVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Ticker")]
    #[serde(rename_all = "camelCase")]
    struct WireTicker {
        id: String,
        symbol: String,
        bid: f64,
        ask: f64,
        last: f64,
        mark: f64,
        #[serde(deserialize_with = "Option::<f64>::deserialize")]
        index: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        mid_price: Option<f64>,
        percentage: f64,
        open_interest: f64,
        funding_rate: f64,
        funding_time: f64,
        volume: f64,
        quote_volume: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        timestamp: Option<f64>,
        #[serde(
            default,
            deserialize_with = "crate::retained_json::deserialize_present_retained_info",
            skip_serializing_if = "Option::is_none"
        )]
        info: Option<crate::retained_json::RetainedJsonInput>,
    }

    impl<'de> serde::Deserialize<'de> for super::Ticker {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            struct InfoVisitor;
            impl<'de> serde::de::Visitor<'de> for InfoVisitor {
                type Value = super::Ticker;
                fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                    f.write_str("a Ticker record")
                }
                fn visit_newtype_struct<D: serde::Deserializer<'de>>(
                    self,
                    d: D,
                ) -> Result<Self::Value, D::Error> {
                    WireTicker::deserialize(crate::strict_decode::object(d))
                }
            }
            deserializer.deserialize_newtype_struct(crate::model_field::INFO_RECORD, InfoVisitor)
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PublicTrade")]
    #[serde(rename_all = "camelCase")]
    struct WirePublicTrade {
        id: String,
        symbol: String,
        price: f64,
        qty: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        quote_qty: Option<f64>,
        timestamp: f64,
        side: OrderSide,
    }

    impl<'de> serde::Deserialize<'de> for super::PublicTrade {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePublicTrade::deserialize(crate::strict_decode::object(deserializer))
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
        market: super::Market,
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
    #[serde(remote = "super::Candle")]
    #[serde(rename_all = "camelCase")]
    struct WireCandle {
        timestamp: f64,
        open: f64,
        high: f64,
        low: f64,
        close: f64,
        volume: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        last_side: Option<OrderSide>,
    }

    impl<'de> serde::Deserialize<'de> for super::Candle {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireCandle::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::Execution")]
    #[serde(rename_all = "camelCase")]
    struct WireExecution {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        id: Option<String>,
        timestamp: f64,
        symbol: String,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        order_id: Option<String>,
        price: f64,
        qty: f64,
        side: OrderSide,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission_asset: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_side: Option<ExecutionPositionSide>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        maker: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        venue_reported_revenue: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        venue_reported_revenue_asset: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::Execution {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExecution::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderBookOrders")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderBookOrders {
        price: f64,
        amount: f64,
        total: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderBookOrders {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderBookOrders::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::OrderBook")]
    #[serde(rename_all = "camelCase")]
    struct WireOrderBook {
        bids: Vec<OrderBookOrders>,
        asks: Vec<OrderBookOrders>,
    }

    impl<'de> serde::Deserialize<'de> for super::OrderBook {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireOrderBook::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::PlaceOrderOpts")]
    #[serde(rename_all = "camelCase")]
    struct WirePlaceOrderOpts {
        symbol: String,
        r#type: OrderType,
        side: OrderSide,
        amount: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trigger_price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        stop_loss: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        take_profit: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        reduce_only: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        time_in_force: Option<OrderTimeInForce>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        tp_trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        sl_trigger_type: Option<OrderStopTriggerType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        client_order_id: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        max_slippage_pct: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        close: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        implied_volatility: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::PlaceOrderOpts {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WirePlaceOrderOpts::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::HistoricalTrade")]
    #[serde(rename_all = "camelCase")]
    struct WireHistoricalTrade {
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        id: Option<String>,
        symbol: String,
        order_id: String,
        price: f64,
        qty: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        quote_qty: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        position_side: ExecutionPositionSide,
        side: OrderSide,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        buyer: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        maker: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        commission_asset: Option<String>,
        time: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::HistoricalTrade {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireHistoricalTrade::deserialize(crate::strict_decode::object(deserializer))
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
    #[serde(remote = "super::HistoricalOrder")]
    #[serde(rename_all = "camelCase")]
    struct WireHistoricalOrder {
        id: String,
        symbol: String,
        side: OrderSide,
        r#type: OrderType,
        status: OrderStatus,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        average_price: Option<f64>,
        amount: f64,
        filled: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        reduce_only: Option<bool>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_side: Option<ExecutionPositionSide>,
        created_time: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        updated_time: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        realized_pnl: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fees: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        fee_asset: Option<String>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        rebate: Option<f64>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        rebate_asset: Option<String>,
    }

    impl<'de> serde::Deserialize<'de> for super::HistoricalOrder {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireHistoricalOrder::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreDataLoaded")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreDataLoaded {
        balance: bool,
        orders: bool,
        markets: bool,
        tickers: bool,
        positions: bool,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreDataLoaded {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreDataLoaded::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::StoreOptions")]
    #[serde(rename_all = "camelCase")]
    struct WireStoreOptions {
        is_hedged: bool,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreOptions::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::FetchedLeverage")]
    #[serde(rename_all = "camelCase")]
    struct WireFetchedLeverage {
        buy_leverage: f64,
        sell_leverage: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        margin_type: Option<MarginType>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        position_mode: Option<PositionMode>,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        is_fetching: Option<bool>,
    }

    impl<'de> serde::Deserialize<'de> for super::FetchedLeverage {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireFetchedLeverage::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::DisplayedMargin")]
    #[serde(rename_all = "camelCase")]
    struct WireDisplayedMargin {
        long_exposure: f64,
        short_exposure: f64,
        total_notional: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::DisplayedMargin {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireDisplayedMargin::deserialize(crate::strict_decode::object(deserializer))
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
        #[serde(rename = "binancespot")]
        Binancespot,
        #[serde(rename = "binancepm")]
        Binancepm,
        #[serde(rename = "bingx")]
        Bingx,
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
        #[serde(rename = "breakoutprop")]
        Breakoutprop,
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
        #[serde(rename = "salsadex")]
        Salsadex,
        #[serde(rename = "tealstreet-dex")]
        TealstreetDex,
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
    #[serde(remote = "super::ExchangeOptionsPolymarket")]
    #[serde(rename_all = "camelCase")]
    struct WireExchangeOptionsPolymarket {
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
        polymarket: Option<crate::ExchangeOptionsPolymarket>,
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
    }

    impl<'de> serde::Deserialize<'de> for super::ExchangeOptions {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireExchangeOptions::deserialize(crate::strict_decode::object(deserializer))
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
    #[serde(remote = "super::UpdateOrderOptsUpdateVariant0")]
    struct WireUpdateOrderOptsUpdateVariant0 {
        #[serde(rename = "amount")]
        amount: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        price: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOptsUpdateVariant0 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOptsUpdateVariant0::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::UpdateOrderOptsUpdateVariant1")]
    struct WireUpdateOrderOptsUpdateVariant1 {
        #[serde(rename = "price")]
        price: f64,
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        amount: Option<f64>,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOptsUpdateVariant1 {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOptsUpdateVariant1::deserialize(crate::strict_decode::object(
                deserializer,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::UpdateOrderOpts")]
    struct WireUpdateOrderOpts {
        #[serde(rename = "order")]
        order: Order,
        #[serde(rename = "update")]
        update: UpdateOrderOptsUpdate,
    }

    impl<'de> serde::Deserialize<'de> for super::UpdateOrderOpts {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireUpdateOrderOpts::deserialize(crate::strict_decode::object(deserializer))
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
    #[serde(remote = "super::DataProvenance")]
    enum WireDataProvenance {
        #[serde(rename = "venue")]
        Venue,
        #[serde(rename = "derived")]
        Derived,
    }

    impl<'de> serde::Deserialize<'de> for super::DataProvenance {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireDataProvenance::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::ClosedPnlEntry")]
    struct WireClosedPnlEntry {
        #[serde(rename = "provenance")]
        provenance: DataProvenance,
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "notional")]
        notional: ClosedPnlEntryNotional,
        #[serde(rename = "entryPrice")]
        entry_price: ClosedPnlEntryEntryPrice,
        #[serde(rename = "exitPrice")]
        exit_price: ClosedPnlEntryExitPrice,
        #[serde(rename = "unrealizedPnl")]
        unrealized_pnl: ClosedPnlEntryUnrealizedPnl,
        #[serde(rename = "side")]
        side: String,
        #[serde(rename = "fees")]
        fees: ClosedPnlEntryFees,
        #[serde(rename = "time")]
        time: f64,
        #[serde(rename = "closedTime")]
        closed_time: f64,
        #[serde(rename = "account")]
        account: String,
        #[serde(rename = "contracts")]
        contracts: ClosedPnlEntryContracts,
        #[serde(rename = "leverage")]
        leverage: ClosedPnlEntryLeverage,
    }

    impl<'de> serde::Deserialize<'de> for super::ClosedPnlEntry {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireClosedPnlEntry::deserialize(crate::strict_decode::object(deserializer))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::IncomeCategory")]
    enum WireIncomeCategory {
        #[serde(rename = "realized_pnl")]
        RealizedPnl,
        #[serde(rename = "funding")]
        Funding,
        #[serde(rename = "commission")]
        Commission,
        #[serde(rename = "rebate")]
        Rebate,
        #[serde(rename = "transfer")]
        Transfer,
        #[serde(rename = "liquidation")]
        Liquidation,
        #[serde(rename = "other")]
        Other,
    }

    impl<'de> serde::Deserialize<'de> for super::IncomeCategory {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            let value = String::deserialize(deserializer)?;
            WireIncomeCategory::deserialize(serde::de::value::StringDeserializer::<D::Error>::new(
                value,
            ))
        }
    }

    #[derive(serde::Deserialize)]
    #[serde(remote = "super::IncomeRecord")]
    struct WireIncomeRecord {
        #[serde(rename = "symbol")]
        symbol: String,
        #[serde(rename = "incomeType")]
        income_type: String,
        #[serde(rename = "income")]
        income: f64,
        #[serde(rename = "asset")]
        asset: String,
        #[serde(rename = "timestamp")]
        timestamp: f64,
        #[serde(rename = "tranId")]
        tran_id: String,
        #[serde(rename = "tradeId")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        trade_id: Option<String>,
        #[serde(rename = "category")]
        #[serde(
            default,
            deserialize_with = "deserialize_present",
            skip_serializing_if = "Option::is_none"
        )]
        category: Option<IncomeCategory>,
    }

    impl<'de> serde::Deserialize<'de> for super::IncomeRecord {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireIncomeRecord::deserialize(crate::strict_decode::object(deserializer))
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
    #[serde(remote = "super::StoreUpdateIds")]
    struct WireStoreUpdateIds {
        #[serde(rename = "positions")]
        positions: f64,
        #[serde(rename = "orders")]
        orders: f64,
    }

    impl<'de> serde::Deserialize<'de> for super::StoreUpdateIds {
        fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
            WireStoreUpdateIds::deserialize(crate::strict_decode::object(deserializer))
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
            crate::strict_decode::tests::string_union::<OrderStatus>(
                "OrderStatus",
                WireOrderStatus::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderStatus::Open)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Closed)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Canceled)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Creating)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Updating)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStatus::Canceling)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderType>(
                "OrderType",
                WireOrderType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderType::Market)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::Limit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::Conditional)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::StopLoss)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::TakeProfit)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderType::TrailingStopLoss)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
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
            crate::strict_decode::tests::string_union::<OrderSide>(
                "OrderSide",
                WireOrderSide::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new("__unknown_inventory__".into()))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderSide::Buy)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderSide::Sell)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderTimeInForce>(
                "OrderTimeInForce",
                WireOrderTimeInForce::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderTimeInForce::GoodTillCancel)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::ImmediateOrCancel)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::FillOrKill)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderTimeInForce::PostOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderStopTriggerType>(
                "OrderStopTriggerType",
                WireOrderStopTriggerType::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderStopTriggerType::MarkPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStopTriggerType::LastPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderStopTriggerType::IndexPrice)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::string_union::<OrderRejectReason>(
                "OrderRejectReason",
                WireOrderRejectReason::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(OrderRejectReason::PostOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderRejectReason::InsufficientBalance)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(OrderRejectReason::ReduceOnly)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
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
            crate::strict_decode::tests::record::<OrderExtra>("OrderExtra");
            crate::strict_decode::tests::record::<Order>("Order");
            crate::strict_decode::tests::record::<PositionExtra>("PositionExtra");
            crate::strict_decode::tests::record::<Position>("Position");
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
            crate::strict_decode::tests::record::<OrderBookOrders>("OrderBookOrders");
            crate::strict_decode::tests::record::<OrderBook>("OrderBook");
            crate::strict_decode::tests::record::<PlaceOrderOpts>("PlaceOrderOpts");
            crate::strict_decode::tests::record::<HistoricalTrade>("HistoricalTrade");
            crate::strict_decode::tests::record::<HistoricalTradeOptions>("HistoricalTradeOptions");
            crate::strict_decode::tests::record::<OHLCVOptions>("OHLCVOptions");
            crate::strict_decode::tests::record::<OHLCVSOptions>("OHLCVSOptions");
            crate::strict_decode::tests::record::<ExecutionsOptions>("ExecutionsOptions");
            crate::strict_decode::tests::record::<OrderHistoryOptions>("OrderHistoryOptions");
            crate::strict_decode::tests::record::<HistoricalOrder>("HistoricalOrder");
            crate::strict_decode::tests::record::<StoreDataLoaded>("StoreDataLoaded");
            crate::strict_decode::tests::record::<StoreOptions>("StoreOptions");
            crate::strict_decode::tests::record::<FetchedLeverage>("FetchedLeverage");
            crate::strict_decode::tests::record::<DisplayedMargin>("DisplayedMargin");
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
                    serde_json::to_value(ExchangeName::Breakoutprop)
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
                    serde_json::to_value(ExchangeName::Salsadex)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(ExchangeName::TealstreetDex)
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
            crate::strict_decode::tests::record::<ExchangeOptions>("ExchangeOptions");
            crate::strict_decode::tests::record::<ValidateAccountResult>("ValidateAccountResult");
            crate::strict_decode::tests::record::<UpdateOrderOptsUpdateVariant0>(
                "UpdateOrderOptsUpdateVariant0",
            );
            crate::strict_decode::tests::record::<UpdateOrderOptsUpdateVariant1>(
                "UpdateOrderOptsUpdateVariant1",
            );
            crate::strict_decode::tests::record::<UpdateOrderOpts>("UpdateOrderOpts");
            crate::strict_decode::tests::record::<ExchangeAccount>("ExchangeAccount");
            crate::strict_decode::tests::string_union::<DataProvenance>(
                "DataProvenance",
                WireDataProvenance::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(DataProvenance::Venue)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(DataProvenance::Derived)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
            crate::strict_decode::tests::record::<ClosedPnlEntry>("ClosedPnlEntry");
            crate::strict_decode::tests::string_union::<IncomeCategory>(
                "IncomeCategory",
                WireIncomeCategory::deserialize(serde::de::value::StringDeserializer::<
                    crate::strict_decode::tests::Inventory,
                >::new(
                    "__unknown_inventory__".into()
                ))
                .err()
                .unwrap()
                .0,
                vec![
                    serde_json::to_value(IncomeCategory::RealizedPnl)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Funding)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Commission)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Rebate)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Transfer)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Liquidation)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                    serde_json::to_value(IncomeCategory::Other)
                        .unwrap()
                        .as_str()
                        .unwrap()
                        .to_owned(),
                ],
            );
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
            crate::strict_decode::tests::record::<StoreUpdateIds>("StoreUpdateIds");
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

impl serde::Serialize for Order {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 22)?;
        state.serialize_field("id", &self.id)?;
        self.parent_id.serialize_field(&mut state, "parentId")?;
        self.status.serialize_field(&mut state, "status")?;
        state.serialize_field("symbol", &self.symbol)?;
        self.r#type.serialize_field(&mut state, "type")?;
        self.side.serialize_field(&mut state, "side")?;
        self.price.serialize_field(&mut state, "price")?;
        state.serialize_field("amount", &self.amount)?;
        state.serialize_field("filled", &self.filled)?;
        state.serialize_field("remaining", &self.remaining)?;
        self.reduce_only.serialize_field(&mut state, "reduceOnly")?;
        self.close.serialize_field(&mut state, "close")?;
        match &self.info {
            ModelField::Absent => {}
            ModelField::Undefined
            | ModelField::Value(crate::retained_json::RetainedJsonInput::Undefined) => {
                state.skip_field("info")?
            }
            ModelField::Value(value) => state.serialize_field("info", value)?,
        }
        self.client_order_id
            .serialize_field(&mut state, "clientOrderId")?;
        self.time_in_force
            .serialize_field(&mut state, "timeInForce")?;
        state.serialize_field("lastUpdate", &self.last_update)?;
        self.trigger_type
            .serialize_field(&mut state, "triggerType")?;
        self.tp_trigger_type
            .serialize_field(&mut state, "tpTriggerType")?;
        self.sl_trigger_type
            .serialize_field(&mut state, "slTriggerType")?;
        self.take_profit.serialize_field(&mut state, "takeProfit")?;
        self.stop_loss.serialize_field(&mut state, "stopLoss")?;
        self.extra.serialize_field(&mut state, "extra")?;
        state.end()
    }
}

impl serde::Serialize for Position {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        use serde::ser::SerializeStruct;
        let mut state = serializer.serialize_struct(crate::model_field::MODEL_RECORD, 17)?;
        serialize_position_fields(self, &mut state)?;
        state.end()
    }
}
fn serialize_position_fields<S: serde::ser::SerializeStruct>(
    position: &Position,
    state: &mut S,
) -> Result<(), S::Error> {
    state.serialize_field("id", &position.id)?;
    state.serialize_field("symbol", &position.symbol)?;
    position.side.serialize_field(state, "side")?;
    state.serialize_field("entryPrice", &position.entry_price)?;
    state.serialize_field("notional", &position.notional)?;
    state.serialize_field("leverage", &position.leverage)?;
    position.margin_type.serialize_field(state, "marginType")?;
    position
        .position_mode
        .serialize_field(state, "positionMode")?;
    state.serialize_field("unrealizedPnl", &position.unrealized_pnl)?;
    position
        .realized_pnl
        .serialize_field(state, "realizedPnl")?;
    state.serialize_field("contracts", &position.contracts)?;
    state.serialize_field("liquidationPrice", &position.liquidation_price)?;
    position.margin_size.serialize_field(state, "marginSize")?;
    match &position.info {
        ModelField::Absent => {}
        ModelField::Undefined
        | ModelField::Value(crate::retained_json::RetainedJsonInput::Undefined) => {
            state.skip_field("info")?
        }
        ModelField::Value(value) => state.serialize_field("info", value)?,
    }
    position.extra.serialize_field(state, "extra")?;
    position.last_update.serialize_field(state, "lastUpdate")?;
    position.option.serialize_field(state, "option")?;
    Ok(())
}

pub mod prepared_order;
pub use prepared_order::PreparedOrderOpts;
