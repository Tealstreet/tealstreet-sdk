//! Binance USD-M wire records. Each struct names only the fields we read;
//! string fields borrow from the frame unless they contain escapes.
use serde::Deserialize;
use std::borrow::Cow;

type Str<'a> = Cow<'a, str>;

use safe_cex_engine::de::{lenient_number, lenient_optional_number};

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolInfo<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default, alias = "contractStatus")]
    pub status: Str<'a>,
    #[serde(borrow)]
    pub base_asset: Str<'a>,
    #[serde(borrow)]
    pub quote_asset: Str<'a>,
    #[serde(borrow, default)]
    pub margin_asset: Str<'a>,
    #[serde(default, deserialize_with = "lenient_number")]
    pub contract_size: f64,
    #[serde(default)]
    pub base_asset_precision: i32,
    #[serde(default)]
    pub quote_asset_precision: i32,
    #[serde(borrow, default)]
    pub contract_type: Str<'a>,
    #[serde(borrow, default)]
    pub filters: Vec<Filter<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Filter<'a> {
    #[serde(borrow)]
    pub filter_type: Str<'a>,
    #[serde(borrow, default)]
    pub tick_size: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub step_size: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub min_qty: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub max_qty: Option<Str<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub last_price: Str<'a>,
    #[serde(borrow)]
    pub price_change_percent: Str<'a>,
    #[serde(borrow)]
    pub volume: Str<'a>,
    #[serde(borrow, default)]
    pub quote_volume: Str<'a>,
    #[serde(borrow, default)]
    pub base_volume: Str<'a>,
    #[serde(borrow, default)]
    pub bid_price: Str<'a>,
    #[serde(borrow, default)]
    pub ask_price: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BookTickerRest<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub bid_price: Str<'a>,
    #[serde(borrow)]
    pub ask_price: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PremiumIndex<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub mark_price: Str<'a>,
    #[serde(borrow)]
    pub index_price: Str<'a>,
    #[serde(borrow)]
    pub last_funding_rate: Str<'a>,
    pub next_funding_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DepthSnapshot<'a> {
    pub last_update_id: u64,
    #[serde(borrow)]
    pub bids: Vec<[Str<'a>; 2]>,
    #[serde(borrow)]
    pub asks: Vec<[Str<'a>; 2]>,
}

/// One element of `!ticker@arr`.
#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub c: Str<'a>,
    #[serde(rename = "P", borrow)]
    pub percentage: Str<'a>,
    #[serde(borrow)]
    pub v: Str<'a>,
    #[serde(borrow, default)]
    pub q: Option<Str<'a>>,
    #[serde(borrow, default, rename = "Q")]
    pub base_volume: Option<Str<'a>>,
    #[serde(borrow, default, rename = "b")]
    pub bid: Option<Str<'a>>,
    #[serde(borrow, default, rename = "a")]
    pub ask: Option<Str<'a>>,
}

/// One element of `!markPrice@arr@1s`.
#[derive(Deserialize)]
pub struct MarkPriceEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub p: Str<'a>,
    #[serde(borrow)]
    pub i: Str<'a>,
    #[serde(borrow)]
    pub r: Str<'a>,
    #[serde(rename = "T")]
    pub funding_time: f64,
}

#[derive(Deserialize)]
pub struct BookTickerEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub b: Str<'a>,
    #[serde(borrow)]
    pub a: Str<'a>,
}

#[derive(Deserialize)]
pub struct DepthEvent<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(rename = "U")]
    pub first_update_id: u64,
    #[serde(rename = "u")]
    pub final_update_id: u64,
    #[serde(default)]
    pub pu: Option<u64>,
    #[serde(borrow, default)]
    pub b: Vec<[Str<'a>; 2]>,
    #[serde(borrow, default)]
    pub a: Vec<[Str<'a>; 2]>,
}

#[derive(Deserialize)]
pub struct KlineEvent<'a> {
    #[serde(borrow)]
    pub k: Kline<'a>,
}

#[derive(Deserialize)]
pub struct Kline<'a> {
    pub t: f64,
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub i: Str<'a>,
    #[serde(borrow)]
    pub o: Str<'a>,
    #[serde(borrow)]
    pub h: Str<'a>,
    #[serde(borrow)]
    pub l: Str<'a>,
    #[serde(borrow)]
    pub c: Str<'a>,
    #[serde(borrow)]
    pub v: Str<'a>,
}

/// REST kline row: `[openTime, open, high, low, close, volume, ...]`.
#[derive(Deserialize)]
pub struct KlineRow<'a>(
    pub f64,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    #[serde(borrow)] pub Str<'a>,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
);

/// Value of the top-level `"e"` field, found without parsing the frame.
pub fn event_type(text: &str) -> Option<&str> {
    let start = text.find("\"e\":\"")? + 5;
    let end = text[start..].find('"')? + start;
    Some(&text[start..end])
}

/// Binance encodes large ids as JSON numbers; they fit in u64.
pub type Id = u64;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListenKey<'a> {
    #[serde(borrow)]
    pub listen_key: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ErrorBody<'a> {
    pub code: i64,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionModeBody {
    pub dual_side_position: bool,
}

#[derive(Deserialize)]
pub struct LeverageBracket<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(default)]
    pub brackets: Vec<Bracket>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bracket {
    #[serde(deserialize_with = "lenient_number")]
    pub initial_leverage: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SymbolConfig<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub margin_type: Str<'a>,
    #[serde(default, deserialize_with = "lenient_number")]
    pub leverage: f64,
}

/// One `positionRisk` row (USD-M `/fapi/v3`, portfolio margin `/papi/v1/um`).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRisk<'a> {
    /// Portfolio margin rows carry the symbol's leverage.
    #[serde(default, deserialize_with = "lenient_optional_number")]
    pub leverage: Option<f64>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub position_side: Str<'a>,
    #[serde(borrow)]
    pub position_amt: Str<'a>,
    #[serde(borrow)]
    pub entry_price: Str<'a>,
    #[serde(borrow)]
    pub un_realized_profit: Str<'a>,
    #[serde(borrow)]
    pub liquidation_price: Str<'a>,
    #[serde(borrow)]
    pub notional: Str<'a>,
    #[serde(borrow, default)]
    pub isolated_margin: Option<Str<'a>>,
    #[serde(default)]
    pub update_time: f64,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Account<'a> {
    #[serde(borrow)]
    pub assets: Vec<AccountAsset<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountAsset<'a> {
    #[serde(borrow)]
    pub asset: Str<'a>,
    #[serde(borrow, default)]
    pub wallet_balance: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub available_balance: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub initial_margin: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub unrealized_profit: Option<Str<'a>>,
}

/// One `/fapi/v1/openOrders` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenOrder<'a> {
    pub order_id: Id,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, rename = "type")]
    pub kind: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow)]
    pub price: Str<'a>,
    #[serde(borrow, default)]
    pub stop_price: Option<Str<'a>>,
    #[serde(borrow)]
    pub orig_qty: Str<'a>,
    #[serde(borrow)]
    pub executed_qty: Str<'a>,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default)]
    pub close_position: bool,
    #[serde(borrow, default)]
    pub working_type: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub time_in_force: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub client_order_id: Option<Str<'a>>,
}

/// One `/fapi/v1/openAlgoOrders` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OpenAlgoOrder<'a> {
    pub algo_id: Id,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub order_type: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow, default)]
    pub trigger_price: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub quantity: Option<Str<'a>>,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(default)]
    pub close_position: bool,
    #[serde(borrow, default)]
    pub working_type: Option<Str<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedOrder {
    #[serde(default)]
    pub order_id: Option<Id>,
    #[serde(default)]
    pub algo_id: Option<Id>,
}

/// One element of a `batchOrders` response: an order or an error.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchItem<'a> {
    #[serde(default)]
    pub order_id: Option<Id>,
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(borrow, default)]
    pub msg: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct OrderUpdateEvent<'a> {
    #[serde(borrow)]
    pub o: OrderUpdate<'a>,
}

/// `ORDER_TRADE_UPDATE.o`.
#[derive(Deserialize)]
pub struct OrderUpdate<'a> {
    pub i: Id,
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(rename = "S", borrow)]
    pub side: Str<'a>,
    /// Original order type; portfolio margin may send only `o`.
    #[serde(borrow, default)]
    pub ot: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub o: Option<Str<'a>>,
    #[serde(rename = "X", borrow)]
    pub status: Str<'a>,
    #[serde(borrow, default)]
    pub x: Option<Str<'a>>,
    #[serde(default)]
    pub t: Option<i64>,
    #[serde(borrow)]
    pub p: Str<'a>,
    #[serde(borrow, default)]
    pub sp: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub ap: Option<Str<'a>>,
    #[serde(borrow)]
    pub q: Str<'a>,
    #[serde(borrow)]
    pub z: Str<'a>,
    #[serde(borrow, default)]
    pub l: Option<Str<'a>>,
    #[serde(rename = "L", borrow, default)]
    pub last_price: Option<Str<'a>>,
    #[serde(rename = "R", default)]
    pub reduce_only: bool,
    #[serde(rename = "cp", default)]
    pub close_position: bool,
    #[serde(borrow, default)]
    pub wt: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub f: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub c: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub r: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct AlgoUpdateEvent<'a> {
    #[serde(borrow)]
    pub o: AlgoUpdate<'a>,
}

/// `ALGO_UPDATE.o`.
#[derive(Deserialize)]
pub struct AlgoUpdate<'a> {
    pub aid: Id,
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(rename = "S", borrow)]
    pub side: Str<'a>,
    #[serde(borrow)]
    pub o: Str<'a>,
    #[serde(rename = "X", borrow)]
    pub status: Str<'a>,
    #[serde(borrow, default)]
    pub tp: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub q: Option<Str<'a>>,
    #[serde(rename = "R", default)]
    pub reduce_only: bool,
    #[serde(default)]
    pub cp: bool,
    #[serde(borrow, default)]
    pub wt: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct AccountUpdateEvent<'a> {
    #[serde(borrow, default)]
    pub fs: Option<Str<'a>>,
    #[serde(borrow)]
    pub a: AccountUpdate<'a>,
}

#[derive(Deserialize)]
pub struct AccountUpdate<'a> {
    #[serde(rename = "B", borrow, default)]
    pub balances: Option<&'a serde_json::value::RawValue>,
    #[serde(rename = "P", borrow, default)]
    pub positions: Vec<AccountPosition<'a>>,
}

#[derive(Deserialize)]
pub struct AccountPosition<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow)]
    pub pa: Str<'a>,
    #[serde(borrow)]
    pub ep: Str<'a>,
    #[serde(borrow)]
    pub up: Str<'a>,
    #[serde(borrow)]
    pub ps: Str<'a>,
    /// Isolated wallet; zero for cross positions.
    #[serde(borrow, default)]
    pub iw: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct ConfigUpdateEvent<'a> {
    #[serde(borrow, default)]
    pub ac: Option<LeverageConfig<'a>>,
}

#[derive(Deserialize)]
pub struct LeverageConfig<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    pub l: f64,
}

/// `/fapi/v1/userTrades` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserTrade<'a> {
    pub id: Id,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub order_id: Id,
    #[serde(borrow)]
    pub price: Str<'a>,
    #[serde(borrow)]
    pub qty: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow, default)]
    pub commission: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub commission_asset: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub position_side: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub realized_pnl: Option<Str<'a>>,
    #[serde(default)]
    pub maker: Option<bool>,
    pub time: f64,
}

/// `/fapi/v1/allOrders` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct HistoricalOrderRow<'a> {
    pub order_id: Id,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub status: Str<'a>,
    #[serde(borrow)]
    pub price: Str<'a>,
    #[serde(borrow, default)]
    pub avg_price: Option<Str<'a>>,
    #[serde(borrow)]
    pub orig_qty: Str<'a>,
    #[serde(borrow)]
    pub executed_qty: Str<'a>,
    #[serde(borrow, rename = "type")]
    pub kind: Str<'a>,
    #[serde(borrow, default)]
    pub orig_type: Option<Str<'a>>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow, default)]
    pub position_side: Option<Str<'a>>,
    #[serde(default)]
    pub reduce_only: bool,
    pub time: f64,
    #[serde(default)]
    pub update_time: f64,
}

/// `/fapi/v1/income` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncomeRow<'a> {
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub income_type: Str<'a>,
    #[serde(borrow)]
    pub income: Str<'a>,
    #[serde(borrow)]
    pub asset: Str<'a>,
    pub time: f64,
    pub tran_id: Id,
    #[serde(borrow, default)]
    pub trade_id: Option<Str<'a>>,
}

/// `/fapi/v1/trades` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicTrade<'a> {
    pub id: Id,
    #[serde(borrow)]
    pub price: Str<'a>,
    #[serde(borrow)]
    pub qty: Str<'a>,
    #[serde(borrow, alias = "baseQty")]
    pub quote_qty: Str<'a>,
    pub time: f64,
    pub is_buyer_maker: bool,
}

/// One `/papi/v1/balance` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmBalance<'a> {
    #[serde(borrow)]
    pub asset: Str<'a>,
    #[serde(borrow, default)]
    pub total_wallet_balance: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub cross_margin_free: Option<Str<'a>>,
    #[serde(borrow, rename = "umUnrealizedPNL", default)]
    pub um_unrealized_pnl: Option<Str<'a>>,
}

/// One `/papi/v1/um/conditional/openOrders` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PmConditionalOrder<'a> {
    #[serde(default)]
    pub strategy_id: Option<Id>,
    #[serde(borrow)]
    pub new_client_strategy_id: Str<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub strategy_type: Str<'a>,
    #[serde(borrow)]
    pub side: Str<'a>,
    #[serde(borrow, default)]
    pub price: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub stop_price: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub orig_qty: Option<Str<'a>>,
    #[serde(default)]
    pub reduce_only: bool,
    #[serde(borrow, default)]
    pub working_type: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct ConditionalUpdateEvent<'a> {
    #[serde(borrow)]
    pub so: ConditionalUpdate<'a>,
}

/// `CONDITIONAL_ORDER_TRADE_UPDATE.so` (portfolio margin).
#[derive(Deserialize)]
pub struct ConditionalUpdate<'a> {
    #[serde(default)]
    pub si: Option<Id>,
    #[serde(borrow)]
    pub s: Str<'a>,
    /// newClientStrategyId.
    #[serde(borrow)]
    pub c: Str<'a>,
    #[serde(rename = "S", borrow)]
    pub side: Str<'a>,
    #[serde(borrow)]
    pub st: Str<'a>,
    #[serde(borrow)]
    pub os: Str<'a>,
    #[serde(borrow, default)]
    pub p: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub sp: Option<Str<'a>>,
    #[serde(borrow, default)]
    pub q: Option<Str<'a>>,
    #[serde(rename = "R", default)]
    pub reduce_only: bool,
    #[serde(borrow, default)]
    pub wt: Option<Str<'a>>,
}

#[derive(Deserialize)]
pub struct SubscriptionAck<'a> {
    #[serde(default)]
    pub id: Option<u64>,
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(borrow, default)]
    pub msg: Str<'a>,
}
