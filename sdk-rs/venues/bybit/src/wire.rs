//! Bybit V5 wire records. Each struct names only the fields we read; string
//! fields borrow from the frame unless they contain escapes. Bybit sends
//! numbers as decimal strings, often empty when not applicable.
use serde::Deserialize;
use std::borrow::Cow;

pub type Str<'a> = Cow<'a, str>;
type Opt<'a> = Option<Str<'a>>;

/// A decimal string as JavaScript's `parseFloat` reads it: empty or malformed is NaN.
pub fn float(text: &str) -> f64 {
    text.trim().parse::<f64>().unwrap_or(f64::NAN)
}

/// An optional decimal string, NaN when absent or empty.
pub fn float_of(value: &Opt<'_>) -> f64 {
    value.as_deref().map_or(f64::NAN, float)
}

/// An optional decimal string where absent, empty or zero-like reads as 0.
pub fn or_zero(value: &Opt<'_>) -> f64 {
    let parsed = float_of(value);
    if parsed.is_finite() { parsed } else { 0.0 }
}

/// A non-empty string field.
pub fn present<'s>(value: &'s Opt<'_>) -> Option<&'s str> {
    value.as_deref().filter(|v| !v.is_empty())
}

/// A boolean Bybit sends bare or as `"true"`.
fn flag<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Flag<'a> {
        Bare(bool),
        Text(#[serde(borrow)] Str<'a>),
        Number(f64),
    }
    Ok(match Option::<Flag>::deserialize(deserializer)? {
        Some(Flag::Bare(value)) => value,
        Some(Flag::Text(text)) => text == "true",
        Some(Flag::Number(value)) => value != 0.0,
        None => false,
    })
}

/// An integer Bybit sends bare or as a decimal string.
fn integer<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Integer<'a> {
        Bare(i64),
        Float(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    Ok(match Option::<Integer>::deserialize(deserializer)? {
        Some(Integer::Bare(value)) => Some(value),
        Some(Integer::Float(value)) => Some(value as i64),
        Some(Integer::Text(text)) => text.trim().parse().ok(),
        None => None,
    })
}

/// The status fields of every REST envelope.
#[derive(Deserialize)]
pub struct Status<'a> {
    #[serde(rename = "retCode", default, deserialize_with = "integer")]
    pub ret_code: Option<i64>,
    #[serde(rename = "retMsg", borrow, default)]
    pub ret_msg: Opt<'a>,
}

/// A streamed list's envelope, its list emptied: status and the page cursor.
#[derive(Deserialize)]
pub struct PageEnvelope<'a> {
    #[serde(borrow, default)]
    pub result: Option<PageCursor<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageCursor<'a> {
    #[serde(borrow, default)]
    pub next_page_cursor: Opt<'a>,
}

/// An envelope whose `result.list` is read whole (short pages only).
#[derive(Deserialize)]
pub struct Listed<T> {
    #[serde(rename = "retCode", default, deserialize_with = "integer")]
    pub ret_code: Option<i64>,
    #[serde(default = "none")]
    pub result: Option<ListResult<T>>,
}

fn none<T>() -> Option<T> {
    None
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ListResult<T> {
    #[serde(default = "Vec::new")]
    pub list: Vec<T>,
    #[serde(default)]
    pub next_page_cursor: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub status: Str<'a>,
    #[serde(borrow, default)]
    pub base_coin: Str<'a>,
    #[serde(borrow, default)]
    pub quote_coin: Str<'a>,
    #[serde(borrow, default)]
    pub delivery_time: Opt<'a>,
    #[serde(borrow, default)]
    pub leverage_filter: Option<LeverageFilter<'a>>,
    #[serde(borrow, default)]
    pub price_filter: Option<PriceFilter<'a>>,
    #[serde(borrow, default)]
    pub lot_size_filter: Option<LotSizeFilter<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LeverageFilter<'a> {
    #[serde(borrow, default)]
    pub min_leverage: Opt<'a>,
    #[serde(borrow, default)]
    pub max_leverage: Opt<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PriceFilter<'a> {
    #[serde(borrow, default)]
    pub tick_size: Opt<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LotSizeFilter<'a> {
    #[serde(borrow, default)]
    pub qty_step: Opt<'a>,
    #[serde(borrow, default)]
    pub min_order_qty: Opt<'a>,
    #[serde(borrow, default)]
    pub max_order_qty: Opt<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeeRate<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub taker_fee_rate: Opt<'a>,
    #[serde(borrow, default)]
    pub maker_fee_rate: Opt<'a>,
}

/// A REST ticker row and, as a socket frame's `data`, a ticker snapshot or
/// delta (a delta omits unchanged fields).
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow<'a> {
    #[serde(borrow, default)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub last_price: Opt<'a>,
    #[serde(borrow, default)]
    pub bid1_price: Opt<'a>,
    #[serde(borrow, default)]
    pub ask1_price: Opt<'a>,
    #[serde(borrow, default)]
    pub mark_price: Opt<'a>,
    #[serde(borrow, default)]
    pub index_price: Opt<'a>,
    #[serde(borrow, default)]
    pub price24h_pcnt: Opt<'a>,
    #[serde(borrow, default)]
    pub open_interest: Opt<'a>,
    #[serde(borrow, default)]
    pub funding_rate: Opt<'a>,
    #[serde(borrow, default)]
    pub next_funding_time: Opt<'a>,
    #[serde(borrow, default)]
    pub volume24h: Opt<'a>,
    #[serde(borrow, default)]
    pub turnover24h: Opt<'a>,
}

#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub data: TickerRow<'a>,
}

#[derive(Deserialize)]
pub struct BookEvent<'a> {
    #[serde(rename = "type", borrow)]
    pub kind: Str<'a>,
    #[serde(borrow)]
    pub data: BookData<'a>,
}

#[derive(Deserialize)]
pub struct BookData<'a> {
    #[serde(borrow)]
    pub s: Str<'a>,
    #[serde(borrow, default)]
    pub b: Vec<[Str<'a>; 2]>,
    #[serde(borrow, default)]
    pub a: Vec<[Str<'a>; 2]>,
    #[serde(default)]
    pub u: u64,
}

#[derive(Deserialize)]
pub struct KlineEvent<'a> {
    #[serde(borrow)]
    pub topic: Str<'a>,
    #[serde(borrow, default)]
    pub data: Vec<WsKline<'a>>,
}

#[derive(Deserialize)]
pub struct WsKline<'a> {
    pub start: f64,
    #[serde(borrow)]
    pub open: Str<'a>,
    #[serde(borrow)]
    pub high: Str<'a>,
    #[serde(borrow)]
    pub low: Str<'a>,
    #[serde(borrow)]
    pub close: Str<'a>,
    #[serde(borrow)]
    pub volume: Str<'a>,
}

/// REST kline row: `[startTime, open, high, low, close, volume, turnover]`.
#[derive(Deserialize)]
pub struct KlineRow<'a>(#[serde(borrow)] pub Vec<Str<'a>>);

/// A control frame: auth and subscribe acknowledgements, pongs.
#[derive(Deserialize)]
pub struct OpFrame<'a> {
    #[serde(borrow, default)]
    pub op: Opt<'a>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(borrow, default)]
    pub ret_msg: Opt<'a>,
}

/// A private topic frame.
#[derive(Deserialize)]
pub struct TopicFrame<T> {
    #[serde(default = "Vec::new")]
    pub data: Vec<T>,
}

/// An order as `/v5/order/realtime`, `/v5/order/history` and the `order`
/// topic report it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow)]
    pub order_id: Str<'a>,
    #[serde(borrow, default)]
    pub order_link_id: Opt<'a>,
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub category: Opt<'a>,
    #[serde(borrow, default)]
    pub price: Opt<'a>,
    #[serde(borrow, default)]
    pub qty: Opt<'a>,
    #[serde(borrow, default)]
    pub side: Opt<'a>,
    #[serde(default, deserialize_with = "integer")]
    pub position_idx: Option<i64>,
    #[serde(borrow, default)]
    pub order_status: Str<'a>,
    #[serde(borrow, default)]
    pub reject_reason: Opt<'a>,
    #[serde(borrow, default)]
    pub avg_price: Opt<'a>,
    #[serde(borrow, default)]
    pub leaves_qty: Opt<'a>,
    #[serde(borrow, default)]
    pub cum_exec_qty: Opt<'a>,
    #[serde(borrow, default)]
    pub time_in_force: Opt<'a>,
    #[serde(borrow, default)]
    pub order_type: Opt<'a>,
    #[serde(borrow, default)]
    pub stop_order_type: Opt<'a>,
    #[serde(borrow, default)]
    pub trigger_price: Opt<'a>,
    #[serde(borrow, default)]
    pub take_profit: Opt<'a>,
    #[serde(borrow, default)]
    pub stop_loss: Opt<'a>,
    #[serde(borrow, default)]
    pub tp_trigger_by: Opt<'a>,
    #[serde(borrow, default)]
    pub sl_trigger_by: Opt<'a>,
    #[serde(borrow, default)]
    pub trigger_by: Opt<'a>,
    #[serde(default, deserialize_with = "flag")]
    pub reduce_only: bool,
    #[serde(default, deserialize_with = "flag")]
    pub close_on_trigger: bool,
    #[serde(borrow, default)]
    pub tpsl_mode: Opt<'a>,
    #[serde(borrow, default)]
    pub create_type: Opt<'a>,
    #[serde(borrow, default)]
    pub created_time: Opt<'a>,
    #[serde(borrow, default)]
    pub updated_time: Opt<'a>,
}

/// A position as `/v5/position/list` and the `position` topic report it.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub category: Opt<'a>,
    #[serde(borrow, default)]
    pub side: Opt<'a>,
    #[serde(borrow, default)]
    pub size: Opt<'a>,
    #[serde(default, deserialize_with = "integer")]
    pub position_idx: Option<i64>,
    #[serde(borrow, default)]
    pub avg_price: Opt<'a>,
    #[serde(borrow, default)]
    pub entry_price: Opt<'a>,
    #[serde(borrow, default)]
    pub position_value: Opt<'a>,
    #[serde(borrow, default)]
    pub leverage: Opt<'a>,
    #[serde(borrow, default)]
    pub unrealised_pnl: Opt<'a>,
    #[serde(borrow, default)]
    pub cur_realised_pnl: Opt<'a>,
    #[serde(borrow, default)]
    pub liq_price: Opt<'a>,
    #[serde(rename = "positionIM", borrow, default)]
    pub position_im: Opt<'a>,
    #[serde(default, deserialize_with = "integer")]
    pub seq: Option<i64>,
    #[serde(borrow, default)]
    pub position_status: Opt<'a>,
    #[serde(borrow, default)]
    pub updated_time: Opt<'a>,
}

/// One execution, from the `execution` topic or `/v5/execution/list`.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecutionRow<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow, default)]
    pub category: Opt<'a>,
    #[serde(borrow)]
    pub order_id: Str<'a>,
    #[serde(borrow, default)]
    pub exec_id: Opt<'a>,
    #[serde(borrow, default)]
    pub side: Opt<'a>,
    #[serde(borrow, default)]
    pub exec_price: Opt<'a>,
    #[serde(borrow, default)]
    pub exec_qty: Opt<'a>,
    #[serde(borrow, default)]
    pub exec_type: Opt<'a>,
    #[serde(borrow, default)]
    pub exec_fee: Opt<'a>,
    #[serde(borrow, default)]
    pub fee_currency: Opt<'a>,
    #[serde(borrow, default)]
    pub exec_time: Opt<'a>,
    #[serde(default)]
    pub is_maker: Option<bool>,
}

/// A unified account, from `/v5/account/wallet-balance` or the `wallet` topic.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletAccount<'a> {
    #[serde(borrow, default)]
    pub total_equity: Opt<'a>,
    #[serde(borrow, default)]
    pub total_wallet_balance: Opt<'a>,
    #[serde(borrow, default)]
    pub total_margin_balance: Opt<'a>,
    #[serde(borrow, default)]
    pub total_available_balance: Opt<'a>,
    #[serde(borrow, default)]
    pub total_initial_margin: Opt<'a>,
    #[serde(borrow, default)]
    pub total_maintenance_margin: Opt<'a>,
    #[serde(borrow, default)]
    pub coin: Vec<WalletCoin<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WalletCoin<'a> {
    #[serde(borrow, default)]
    pub equity: Opt<'a>,
    #[serde(borrow, default)]
    pub usd_value: Opt<'a>,
    #[serde(borrow, default)]
    pub wallet_balance: Opt<'a>,
    #[serde(rename = "totalPositionIM", borrow, default)]
    pub total_position_im: Opt<'a>,
    #[serde(rename = "totalOrderIM", borrow, default)]
    pub total_order_im: Opt<'a>,
    #[serde(borrow, default)]
    pub locked: Opt<'a>,
    #[serde(borrow, default)]
    pub bonus: Opt<'a>,
}

#[derive(Deserialize)]
pub struct WalletEnvelope<'a> {
    #[serde(borrow, default)]
    pub result: Option<WalletResult<'a>>,
}

#[derive(Deserialize)]
pub struct WalletResult<'a> {
    #[serde(borrow, default)]
    pub list: Vec<WalletAccount<'a>>,
}

#[derive(Deserialize)]
pub struct AccountInfoEnvelope<'a> {
    #[serde(borrow, default)]
    pub result: Option<AccountInfo<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountInfo<'a> {
    #[serde(borrow, default)]
    pub margin_mode: Opt<'a>,
    #[serde(default, deserialize_with = "integer")]
    pub unified_margin_status: Option<i64>,
}

/// `/v5/order/create` and `/v5/order/amend`.
#[derive(Deserialize)]
pub struct Placed<'a> {
    #[serde(borrow, default)]
    pub result: Option<PlacedResult<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PlacedResult<'a> {
    #[serde(borrow, default)]
    pub order_id: Opt<'a>,
}

/// `/v5/order/create-batch` and `/v5/order/cancel-batch`: one result and one
/// status per request, in request order.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch<'a> {
    #[serde(borrow, default)]
    pub result: Option<BatchResult<'a>>,
    #[serde(borrow, default)]
    pub ret_ext_info: Option<BatchStatuses<'a>>,
}

#[derive(Deserialize)]
pub struct BatchResult<'a> {
    #[serde(borrow, default)]
    pub list: Vec<PlacedResult<'a>>,
}

#[derive(Deserialize)]
pub struct BatchStatuses<'a> {
    #[serde(borrow, default)]
    pub list: Vec<BatchStatus<'a>>,
}

#[derive(Deserialize)]
pub struct BatchStatus<'a> {
    #[serde(default)]
    pub code: i64,
    #[serde(borrow, default)]
    pub msg: Opt<'a>,
}

/// `/v5/account/set-margin-mode` explains a refusal in `result.reasons`.
#[derive(Deserialize)]
pub struct MarginModeResult<'a> {
    #[serde(borrow, default)]
    pub result: Option<MarginModeReasons<'a>>,
}

#[derive(Deserialize)]
pub struct MarginModeReasons<'a> {
    #[serde(borrow, default)]
    pub reasons: Vec<MarginModeReason<'a>>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MarginModeReason<'a> {
    #[serde(borrow, default)]
    pub reason_msg: Opt<'a>,
}

/// `/v5/account/transaction-log` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRow<'a> {
    #[serde(borrow, default)]
    pub symbol: Opt<'a>,
    #[serde(rename = "type", borrow, default)]
    pub kind: Opt<'a>,
    #[serde(borrow, default)]
    pub currency: Opt<'a>,
    #[serde(borrow, default)]
    pub transaction_time: Opt<'a>,
    #[serde(borrow, default)]
    pub cash_flow: Opt<'a>,
    #[serde(borrow, default)]
    pub funding: Opt<'a>,
    #[serde(borrow, default)]
    pub fee: Opt<'a>,
    #[serde(borrow, default)]
    pub trade_id: Opt<'a>,
    #[serde(borrow, default)]
    pub order_id: Opt<'a>,
}

/// `/v5/market/recent-trade` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecentTrade<'a> {
    #[serde(borrow, default)]
    pub exec_id: Opt<'a>,
    #[serde(borrow, default)]
    pub price: Opt<'a>,
    #[serde(borrow, default)]
    pub size: Opt<'a>,
    #[serde(borrow, default)]
    pub side: Opt<'a>,
    #[serde(borrow, default)]
    pub time: Opt<'a>,
}

/// Value of a frame's top-level `"topic"`, found without parsing the frame.
pub fn topic(text: &str) -> Option<&str> {
    let start = text.find("\"topic\":\"")? + 9;
    let end = text[start..].find('"')? + start;
    Some(&text[start..end])
}
