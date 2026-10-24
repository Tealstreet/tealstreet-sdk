//! Store-facing records. Field names and enum spellings serialize to exactly the
//! shapes `packages/safe-cex/src/types.ts` gives the main thread.
use serde::{Deserialize, Serialize};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "camelCase")]
pub enum OrderIdentityKind {
    ClientOrderId,
    ClientOrderIndex,
    OrderCode,
}

/// Native identity selected for a write; this is not a venue acknowledgement.
#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct OrderIdentityReceipt {
    pub symbol: String,
    pub kind: OrderIdentityKind,
    pub identity: String,
}

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

/// A candle interval: the TS `AnyTimeframe` spellings, parsed once where a
/// command enters the engine. Venues map it to their own codes and reject
/// what they cannot serve instead of substituting another interval.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Interval {
    S1,
    S5,
    S15,
    S30,
    M1,
    M3,
    M5,
    M15,
    M30,
    H1,
    H2,
    H3,
    H4,
    H5,
    H6,
    H7,
    H8,
    H9,
    H10,
    H11,
    H12,
    H24,
    D1,
    D2,
    D3,
    W1,
    Mo1,
}

impl Interval {
    pub const ALL: [Self; 27] = [
        Self::S1,
        Self::S5,
        Self::S15,
        Self::S30,
        Self::M1,
        Self::M3,
        Self::M5,
        Self::M15,
        Self::M30,
        Self::H1,
        Self::H2,
        Self::H3,
        Self::H4,
        Self::H5,
        Self::H6,
        Self::H7,
        Self::H8,
        Self::H9,
        Self::H10,
        Self::H11,
        Self::H12,
        Self::H24,
        Self::D1,
        Self::D2,
        Self::D3,
        Self::W1,
        Self::Mo1,
    ];

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|i| i.as_str() == text)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::S1 => "1s",
            Self::S5 => "5s",
            Self::S15 => "15s",
            Self::S30 => "30s",
            Self::M1 => "1m",
            Self::M3 => "3m",
            Self::M5 => "5m",
            Self::M15 => "15m",
            Self::M30 => "30m",
            Self::H1 => "1h",
            Self::H2 => "2h",
            Self::H3 => "3h",
            Self::H4 => "4h",
            Self::H5 => "5h",
            Self::H6 => "6h",
            Self::H7 => "7h",
            Self::H8 => "8h",
            Self::H9 => "9h",
            Self::H10 => "10h",
            Self::H11 => "11h",
            Self::H12 => "12h",
            Self::H24 => "24h",
            Self::D1 => "1d",
            Self::D2 => "2d",
            Self::D3 => "3d",
            Self::W1 => "1w",
            Self::Mo1 => "1M",
        }
    }

    /// Nominal length; a month counts as 30 days, as the TS SDK's
    /// `timeframeToSeconds` does. Calendar-exact windows use `time::sub_months`.
    pub fn ms(self) -> f64 {
        const S: f64 = 1000.0;
        const M: f64 = 60.0 * S;
        const H: f64 = 60.0 * M;
        const D: f64 = 24.0 * H;
        match self {
            Self::S1 => S,
            Self::S5 => 5.0 * S,
            Self::S15 => 15.0 * S,
            Self::S30 => 30.0 * S,
            Self::M1 => M,
            Self::M3 => 3.0 * M,
            Self::M5 => 5.0 * M,
            Self::M15 => 15.0 * M,
            Self::M30 => 30.0 * M,
            Self::H1 => H,
            Self::H2 => 2.0 * H,
            Self::H3 => 3.0 * H,
            Self::H4 => 4.0 * H,
            Self::H5 => 5.0 * H,
            Self::H6 => 6.0 * H,
            Self::H7 => 7.0 * H,
            Self::H8 => 8.0 * H,
            Self::H9 => 9.0 * H,
            Self::H10 => 10.0 * H,
            Self::H11 => 11.0 * H,
            Self::H12 => 12.0 * H,
            Self::H24 | Self::D1 => D,
            Self::D2 => 2.0 * D,
            Self::D3 => 3.0 * D,
            Self::W1 => 7.0 * D,
            Self::Mo1 => 30.0 * D,
        }
    }

    /// The error a venue returns for an interval it cannot serve.
    pub fn unsupported(self, venue: &str) -> String {
        format!("{venue} does not support {} candles", self.as_str())
    }
}

impl std::fmt::Display for Interval {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for Interval {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let text = std::borrow::Cow::<str>::deserialize(d)?;
        Self::parse(&text).ok_or_else(|| {
            serde::de::Error::custom(format!("Unsupported candle interval {text:?}"))
        })
    }
}

impl Serialize for Interval {
    fn serialize<S: serde::Serializer>(&self, s: S) -> Result<S::Ok, S::Error> {
        s.serialize_str(self.as_str())
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

impl Precision {
    /// Orders round to these steps; a market without two positive finite steps is malformed.
    pub fn is_tradable(&self) -> bool {
        [self.amount, self.price]
            .iter()
            .all(|step| step.is_finite() && *step > 0.0)
    }
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

/// The venue token behind a market whose display name differs from it, such as
/// Hyperliquid's Unit-bridged `UBTC` behind `BTC/USDC`: a risk disclosure.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UnderlyingToken {
    pub market: u32,
    pub name: String,
    pub full_name: Option<String>,
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
    #[serde(default, deserialize_with = "replayed_enum::optional")]
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
    #[serde(rename = "type", deserialize_with = "replayed_enum::required")]
    pub kind: OrderType,
    #[serde(deserialize_with = "replayed_enum::required")]
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
    #[serde(default, deserialize_with = "replayed_enum::optional")]
    pub time_in_force: Option<TimeInForce>,
    #[serde(default, deserialize_with = "replayed_enum::optional")]
    pub trigger_type: Option<StopTriggerType>,
    #[serde(default, deserialize_with = "replayed_enum::optional")]
    pub tp_trigger_type: Option<StopTriggerType>,
    #[serde(default, deserialize_with = "replayed_enum::optional")]
    pub sl_trigger_type: Option<StopTriggerType>,
    #[serde(
        default,
        deserialize_with = "caller_client_order_id",
        skip_serializing_if = "Option::is_none"
    )]
    pub client_order_id: Option<String>,
    #[serde(default)]
    pub close: Option<bool>,
    #[serde(default, deserialize_with = "replayed_enum::optional")]
    pub stp_mode: Option<StpMode>,
    #[serde(default)]
    pub bybit_bbo: Option<BybitBbo>,
    #[serde(default)]
    pub bybit_slippage_tolerance_ticks: Option<f64>,
    #[serde(default)]
    pub implied_volatility: Option<f64>,
}

impl PlaceOrderOpts {
    /// The app sends protective orders with `triggerPrice` equal to `price`;
    /// venues that trigger on `price` accept that redundancy and nothing else.
    pub fn redundant_protective_trigger(&self) -> bool {
        matches!(
            self.kind,
            OrderType::StopLoss | OrderType::TakeProfit | OrderType::TrailingStopLoss
        ) && self.trigger_price.is_some()
            && self.trigger_price == self.price
    }
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
    fn only_two_positive_finite_steps_are_tradable() {
        let precision = |amount, price| Precision { amount, price };
        assert!(precision(0.001, 0.1).is_tradable());
        for (amount, price) in [
            (0.0, 0.1),
            (0.001, -0.1),
            (f64::NAN, 0.1),
            (0.001, f64::INFINITY),
        ] {
            assert!(!precision(amount, price).is_tradable(), "{amount} {price}");
        }
    }

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

    #[test]
    fn only_protective_orders_with_matching_prices_carry_a_redundant_trigger() {
        let parse = |kind: &str, fields: &str| {
            serde_json::from_str::<PlaceOrderOpts>(&format!(
                r#"{{"symbol":"BTCUSDT","type":"{kind}","side":"sell","amount":1{fields}}}"#
            ))
            .unwrap()
            .redundant_protective_trigger()
        };
        let app = r#","price":95,"triggerPrice":95"#;
        assert!(parse("stop_market", app));
        assert!(parse("take_profit_market", app));
        assert!(parse("trailing_stop_market", app));
        assert!(!parse("stop_market", r#","price":95,"triggerPrice":94"#));
        assert!(!parse("stop_market", r#","triggerPrice":95"#));
        assert!(!parse("stop_market", r#","price":95"#));
        assert!(!parse("limit", app));
        assert!(!parse("conditional", app));
    }
}

/// Command wire enums decode as serde's buffered internally tagged command
/// enum did: a string names the variant, a single-key map with a null value
/// is the variant's map form, and anything else is `invalid type: X, expected
/// string or map`. Only command payload fields use this; the enums' own
/// `Deserialize` stays unchanged for every other caller.
pub(crate) mod replayed_enum {
    use serde::de::{self, Deserialize, Deserializer, IntoDeserializer, Unexpected, Visitor};
    use std::fmt;
    use std::marker::PhantomData;

    pub(crate) fn required<'de, D, E>(deserializer: D) -> Result<E, D::Error>
    where
        D: Deserializer<'de>,
        E: Deserialize<'de>,
    {
        deserializer.deserialize_any(Replayed(PhantomData))
    }

    pub(crate) fn optional<'de, D, E>(deserializer: D) -> Result<Option<E>, D::Error>
    where
        D: Deserializer<'de>,
        E: Deserialize<'de>,
    {
        struct Optional<E>(PhantomData<E>);
        impl<'de, E: Deserialize<'de>> Visitor<'de> for Optional<E> {
            type Value = Option<E>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("option")
            }
            fn visit_none<X: de::Error>(self) -> Result<Option<E>, X> {
                Ok(None)
            }
            fn visit_unit<X: de::Error>(self) -> Result<Option<E>, X> {
                Ok(None)
            }
            fn visit_some<D: Deserializer<'de>>(self, d: D) -> Result<Option<E>, D::Error> {
                required(d).map(Some)
            }
        }
        deserializer.deserialize_option(Optional(PhantomData))
    }

    struct Replayed<E>(PhantomData<E>);

    /// Just enough of a value to name it in a type error.
    enum Shape {
        Unit,
        Bool(bool),
        Signed(i64),
        Unsigned(u64),
        Float(f64),
        Str(String),
        Seq,
        Map,
    }

    impl Shape {
        fn unexpected(&self) -> Option<Unexpected<'_>> {
            Some(match self {
                Shape::Unit => return None,
                Shape::Bool(v) => Unexpected::Bool(*v),
                Shape::Signed(v) => Unexpected::Signed(*v),
                Shape::Unsigned(v) => Unexpected::Unsigned(*v),
                Shape::Float(v) => Unexpected::Float(*v),
                Shape::Str(v) => Unexpected::Str(v),
                Shape::Seq => Unexpected::Seq,
                Shape::Map => Unexpected::Map,
            })
        }
    }

    impl<'de> Deserialize<'de> for Shape {
        fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Shape, D::Error> {
            struct ShapeVisitor;
            impl<'de> Visitor<'de> for ShapeVisitor {
                type Value = Shape;
                fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                    f.write_str("any value")
                }
                fn visit_unit<X>(self) -> Result<Shape, X> {
                    Ok(Shape::Unit)
                }
                fn visit_bool<X>(self, v: bool) -> Result<Shape, X> {
                    Ok(Shape::Bool(v))
                }
                fn visit_i64<X>(self, v: i64) -> Result<Shape, X> {
                    Ok(Shape::Signed(v))
                }
                fn visit_u64<X>(self, v: u64) -> Result<Shape, X> {
                    Ok(Shape::Unsigned(v))
                }
                fn visit_f64<X>(self, v: f64) -> Result<Shape, X> {
                    Ok(Shape::Float(v))
                }
                fn visit_str<X>(self, v: &str) -> Result<Shape, X> {
                    Ok(Shape::Str(v.to_owned()))
                }
                fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<Shape, A::Error> {
                    while seq.next_element::<de::IgnoredAny>()?.is_some() {}
                    Ok(Shape::Seq)
                }
                fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Shape, A::Error> {
                    while map
                        .next_entry::<de::IgnoredAny, de::IgnoredAny>()?
                        .is_some()
                    {}
                    Ok(Shape::Map)
                }
            }
            deserializer.deserialize_any(ShapeVisitor)
        }
    }

    impl<'de, E: Deserialize<'de>> Visitor<'de> for Replayed<E> {
        type Value = E;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("string or map")
        }
        fn visit_str<X: de::Error>(self, v: &str) -> Result<E, X> {
            E::deserialize(v.to_owned().into_deserializer())
        }
        fn visit_bool<X: de::Error>(self, v: bool) -> Result<E, X> {
            Err(X::invalid_type(Unexpected::Bool(v), &self))
        }
        fn visit_i64<X: de::Error>(self, v: i64) -> Result<E, X> {
            Err(X::invalid_type(Unexpected::Signed(v), &self))
        }
        fn visit_u64<X: de::Error>(self, v: u64) -> Result<E, X> {
            Err(X::invalid_type(Unexpected::Unsigned(v), &self))
        }
        fn visit_f64<X: de::Error>(self, v: f64) -> Result<E, X> {
            Err(X::invalid_type(Unexpected::Float(v), &self))
        }
        fn visit_unit<X: de::Error>(self) -> Result<E, X> {
            Err(X::invalid_type(Unexpected::Unit, &self))
        }
        fn visit_seq<A: de::SeqAccess<'de>>(self, mut seq: A) -> Result<E, A::Error> {
            while seq.next_element::<de::IgnoredAny>()?.is_some() {}
            Err(de::Error::invalid_type(Unexpected::Seq, &self))
        }
        fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<E, A::Error> {
            // The buffer checked the entry count, then the variant name, then
            // that the variant's value was unit.
            let mut first: Option<(String, Shape)> = None;
            let mut entries = 0usize;
            while let Some(key) = map.next_key::<String>()? {
                let shape = map.next_value::<Shape>()?;
                entries += 1;
                first.get_or_insert((key, shape));
            }
            match first {
                Some((variant, shape)) if entries == 1 => {
                    let value = E::deserialize(variant.into_deserializer())?;
                    match shape.unexpected() {
                        None => Ok(value),
                        Some(unexpected) => Err(de::Error::invalid_type(unexpected, &"unit")),
                    }
                }
                _ => Err(de::Error::invalid_value(
                    Unexpected::Map,
                    &"map with a single key",
                )),
            }
        }
    }
}
