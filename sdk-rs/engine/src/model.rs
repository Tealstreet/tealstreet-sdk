//! Store-facing records. Field names and enum spellings serialize to exactly the
//! shapes `packages/safe-cex/src/types.ts` gives the main thread.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderSide {
    Buy,
    Sell,
}

impl OrderSide {
    pub fn opposite(self) -> Self {
        match self {
            Self::Buy => Self::Sell,
            Self::Sell => Self::Buy,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum OrderStatus {
    Open,
    Closed,
    Canceled,
    Creating,
    Updating,
    Canceling,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum TimeInForce {
    GoodTillCancel,
    ImmediateOrCancel,
    FillOrKill,
    PostOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StopTriggerType {
    MarkPrice,
    LastPrice,
    IndexPrice,
}

/// Account or per-order self-trade prevention policy where supported.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum StpMode {
    ExpireTaker,
    ExpireMaker,
    ExpireBoth,
}

impl StpMode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ExpireTaker => "EXPIRE_TAKER",
            Self::ExpireMaker => "EXPIRE_MAKER",
            Self::ExpireBoth => "EXPIRE_BOTH",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum RejectReason {
    PostOnly,
    InsufficientBalance,
    ReduceOnly,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionSide {
    Long,
    Short,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PositionMode {
    Hedged,
    Oneway,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MarginType {
    Isolated,
    Cross,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct MinMax {
    pub min: f64,
    pub max: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Min {
    pub min: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Precision {
    pub amount: f64,
    pub price: f64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
pub struct Limits {
    pub amount: MinMax,
    pub leverage: MinMax,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub price: Option<Min>,
}

/// Native semantic labels; an absent label differs from an explicitly empty one.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstrumentClassification {
    pub market: u32,
    pub symbol_type: Option<String>,
    pub market_region: Option<String>,
    pub underlying_ticker: Option<String>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProductKind {
    Future,
    Call,
    Put,
}

/// Native derivative terms. Unknown expiry/settlement remain absent.
#[derive(Clone, Debug, PartialEq)]
pub struct DerivativeProduct {
    pub market: u32,
    pub kind: ProductKind,
    pub underlying: String,
    pub expiry: Option<f64>,
    pub strike: Option<f64>,
    pub settlement_asset: Option<String>,
    pub premium_asset: Option<String>,
    pub style: Option<String>,
    pub settlement: Option<String>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct OptionGreeks {
    pub delta: f64,
    pub gamma: f64,
    pub vega: f64,
    pub theta: f64,
    pub units: String,
}

/// Position-specific Greeks keyed by position ID, never only by market.
#[derive(Clone, Debug, PartialEq)]
pub struct PositionGreeks {
    pub position_id: String,
    pub greeks: OptionGreeks,
}

#[derive(Clone, Debug, PartialEq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OptionChainQuery {
    pub underlying: String,
    pub expiry: Option<f64>,
    pub right: Option<OptionRight>,
    pub strike_min: Option<f64>,
    pub strike_max: Option<f64>,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum OptionRight {
    Call,
    Put,
}

/// Typed outcome metadata for prediction markets; ordinary venues allocate none.
/// `market` is an index in the store's current market table.
#[derive(Clone, Debug, PartialEq)]
pub struct PredictionMarket {
    pub market: u32,
    pub condition_id: String,
    pub outcome: String,
    pub question: String,
    pub description: String,
    pub end_date: String,
    pub slug: String,
    pub tick_size: f64,
    pub neg_risk: bool,
}

/// Settlement capabilities of one prediction outcome holding.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PredictionHolding {
    pub market: u32,
    pub redeemable: bool,
    pub mergeable: bool,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Market {
    pub id: String,
    pub symbol: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pretty_symbol: Option<String>,
    pub base: String,
    pub quote: String,
    pub active: bool,
    pub precision: Precision,
    pub limits: Limits,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub contract_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_expiring: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_inverse: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_spot: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_perpetual: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub is_read_only: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub taker_fee: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub maker_fee: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Ticker {
    pub id: String,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub mark: f64,
    pub index: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub mid_price: Option<f64>,
    pub percentage: f64,
    pub open_interest: f64,
    pub funding_rate: f64,
    pub funding_time: f64,
    pub volume: f64,
    pub quote_volume: f64,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderExtra {
    #[serde(skip)]
    pub take_profit_known: bool,
    #[serde(skip)]
    pub stop_loss_known: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tp_trigger_type: Option<StopTriggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sl_trigger_type: Option<StopTriggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_native: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub filled_in_placement_units: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub remaining_notional: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conditional_order_price: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reject_reason: Option<RejectReason>,
}

/// Order identity supplied by the caller when cancellation precedes reconciliation.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct CancelOrderRef {
    pub id: String,
    pub symbol: String,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Order {
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub parent_id: Option<String>,
    pub status: OrderStatus,
    pub symbol: String,
    #[serde(rename = "type")]
    pub kind: OrderType,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub filled: f64,
    pub remaining: f64,
    pub reduce_only: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub close: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_order_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub time_in_force: Option<TimeInForce>,
    pub last_update: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub trigger_type: Option<StopTriggerType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub take_profit: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub stop_loss: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub extra: Option<OrderExtra>,
}

#[derive(Clone, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Position {
    pub id: String,
    pub symbol: String,
    pub side: PositionSide,
    pub entry_price: f64,
    pub notional: f64,
    pub leverage: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_type: Option<MarginType>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub position_mode: Option<PositionMode>,
    pub unrealized_pnl: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub realized_pnl: Option<f64>,
    pub contracts: f64,
    pub liquidation_price: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub margin_size: Option<f64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_update: Option<f64>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize)]
pub struct Balance {
    pub used: f64,
    pub free: f64,
    pub total: f64,
    pub upnl: f64,
}

/// Collateral holdings alongside the account's aggregate balance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CoinBalance {
    pub base: String,
    pub amount: f64,
    pub notional: f64,
}

/// Full per-currency account balances, alongside the aggregate balance.
#[derive(Clone, Debug, PartialEq, Serialize)]
pub struct CurrencyBalance {
    pub currency: String,
    pub balance: Balance,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct Loaded {
    pub balance: bool,
    pub orders: bool,
    pub markets: bool,
    pub tickers: bool,
    pub positions: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize)]
pub struct UpdateIds {
    pub positions: u64,
    pub orders: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Candle {
    pub timestamp: f64,
    pub open: f64,
    pub high: f64,
    pub low: f64,
    pub close: f64,
    pub volume: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BybitBbo {
    pub side_type: String,
    pub level: f64,
}

#[derive(Clone, Debug, PartialEq, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PlaceOrderOpts {
    pub symbol: String,
    #[serde(rename = "type")]
    pub kind: OrderType,
    pub side: OrderSide,
    pub amount: f64,
    #[serde(default)]
    pub price: Option<f64>,
    #[serde(default)]
    pub trigger_price: Option<f64>,
    /// A venue's protected market-order slippage, in percentage points.
    #[serde(default)]
    pub max_slippage_pct: Option<f64>,
    #[serde(default)]
    pub stop_loss: Option<f64>,
    #[serde(default)]
    pub take_profit: Option<f64>,
    #[serde(default)]
    pub reduce_only: Option<bool>,
    #[serde(default)]
    pub time_in_force: Option<TimeInForce>,
    #[serde(default)]
    pub trigger_type: Option<StopTriggerType>,
    #[serde(default)]
    pub tp_trigger_type: Option<StopTriggerType>,
    #[serde(default)]
    pub sl_trigger_type: Option<StopTriggerType>,
    #[serde(
        default,
        deserialize_with = "caller_client_order_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub close: Option<bool>,
    #[serde(default)]
    pub stp_mode: Option<StpMode>,
    #[serde(default)]
    pub bybit_bbo: Option<BybitBbo>,
    #[serde(default)]
    pub bybit_slippage_tolerance_ticks: Option<f64>,
    #[serde(default)]
    pub implied_volatility: Option<f64>,
}

fn caller_client_order_id<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn order_options_round_trip_without_turning_absent_identity_into_caller_null() {
        for id in ["", r#","clientOrderId":"caller-1""#] {
            let json =
                format!(r#"{{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1{id}}}"#);
            let opts: PlaceOrderOpts = serde_json::from_str(&json).unwrap();
            let encoded = serde_json::to_string(&opts).unwrap();
            assert_eq!(
                serde_json::from_str::<PlaceOrderOpts>(&encoded).unwrap(),
                opts
            );
        }
        assert!(serde_json::from_str::<PlaceOrderOpts>(
            r#"{"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"clientOrderId":null}"#,
        ).is_err());
    }
}
