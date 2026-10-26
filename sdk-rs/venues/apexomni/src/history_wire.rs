//! Typed execution aliases used by the Omni v3 and legacy history routes.
use serde::{Deserialize, Deserializer};
use std::borrow::Cow;
fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Option<String>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Field<'a> {
        Text(Cow<'a, str>),
        Integer(i64),
        Unsigned(u64),
        Number(f64),
    }
    Ok(Option::<Field<'_>>::deserialize(d)?.map(|v| match v {
        Field::Text(s) => s.into_owned(),
        Field::Integer(n) => n.to_string(),
        Field::Unsigned(n) => n.to_string(),
        Field::Number(n) => n.to_string(),
    }))
}
#[derive(Default, Deserialize)]
#[serde(default)]
pub struct Execution {
    #[serde(rename = "symbol", deserialize_with = "text")]
    pub symbol: Option<String>,
    #[serde(rename = "s", deserialize_with = "text")]
    pub s: Option<String>,
    #[serde(rename = "instId", deserialize_with = "text")]
    pub inst_id: Option<String>,
    #[serde(rename = "instrument", deserialize_with = "text")]
    pub instrument: Option<String>,
    #[serde(rename = "market", deserialize_with = "text")]
    pub market: Option<String>,
    #[serde(rename = "crossSymbolName", deserialize_with = "text")]
    pub cross_symbol_name: Option<String>,
    #[serde(rename = "marketSymbol", deserialize_with = "text")]
    pub market_symbol: Option<String>,
    #[serde(rename = "side", deserialize_with = "text")]
    pub side: Option<String>,
    #[serde(rename = "S", deserialize_with = "text")]
    pub side_compact: Option<String>,
    #[serde(rename = "cumSuccessFillValue", deserialize_with = "text")]
    pub cum_success_fill_value: Option<String>,
    #[serde(rename = "cumMatchFillValue", deserialize_with = "text")]
    pub cum_match_fill_value: Option<String>,
    #[serde(rename = "cumSuccessFillSize", deserialize_with = "text")]
    pub cum_success_fill_size: Option<String>,
    #[serde(rename = "cumMatchFillSize", deserialize_with = "text")]
    pub cum_match_fill_size: Option<String>,
    #[serde(rename = "fillPrice", deserialize_with = "text")]
    pub fill_price: Option<String>,
    #[serde(rename = "fillPx", deserialize_with = "text")]
    pub fill_px: Option<String>,
    #[serde(rename = "execPrice", deserialize_with = "text")]
    pub exec_price: Option<String>,
    #[serde(rename = "latestMatchFillPrice", deserialize_with = "text")]
    pub latest_match_fill_price: Option<String>,
    #[serde(rename = "avgPrice", deserialize_with = "text")]
    pub avg_price: Option<String>,
    #[serde(rename = "price", deserialize_with = "text")]
    pub price: Option<String>,
    #[serde(rename = "p", deserialize_with = "text")]
    pub p: Option<String>,
    #[serde(rename = "fillSize", deserialize_with = "text")]
    pub fill_size: Option<String>,
    #[serde(rename = "execQty", deserialize_with = "text")]
    pub exec_qty: Option<String>,
    #[serde(rename = "matchFillSize", deserialize_with = "text")]
    pub match_fill_size: Option<String>,
    #[serde(rename = "lastFillSize", deserialize_with = "text")]
    pub last_fill_size: Option<String>,
    #[serde(rename = "qty", deserialize_with = "text")]
    pub qty: Option<String>,
    #[serde(rename = "q", deserialize_with = "text")]
    pub q: Option<String>,
    #[serde(rename = "amount", deserialize_with = "text")]
    pub amount: Option<String>,
    #[serde(rename = "size", deserialize_with = "text")]
    pub size: Option<String>,
    #[serde(rename = "v", deserialize_with = "text")]
    pub v: Option<String>,
    #[serde(rename = "createdAt", deserialize_with = "text")]
    pub created_at: Option<String>,
    #[serde(rename = "fillTime", deserialize_with = "text")]
    pub fill_time: Option<String>,
    #[serde(rename = "createdTime", deserialize_with = "text")]
    pub created_time: Option<String>,
    #[serde(rename = "timestamp", deserialize_with = "text")]
    pub timestamp: Option<String>,
    #[serde(rename = "ts", deserialize_with = "text")]
    pub ts: Option<String>,
    #[serde(rename = "timeStamp", deserialize_with = "text")]
    pub time_stamp: Option<String>,
    #[serde(rename = "time", deserialize_with = "text")]
    pub time: Option<String>,
    #[serde(rename = "updatedAt", deserialize_with = "text")]
    pub updated_at: Option<String>,
    #[serde(rename = "updatedTime", deserialize_with = "text")]
    pub updated_time: Option<String>,
    #[serde(rename = "tradeTime", deserialize_with = "text")]
    pub trade_time: Option<String>,
    #[serde(rename = "transactTime", deserialize_with = "text")]
    pub transact_time: Option<String>,
    #[serde(rename = "E", deserialize_with = "text")]
    pub e: Option<String>,
    #[serde(rename = "t", deserialize_with = "text")]
    pub t: Option<String>,
    #[serde(rename = "T", deserialize_with = "text")]
    pub t_upper: Option<String>,
    #[serde(rename = "orderId", deserialize_with = "text")]
    pub order_id: Option<String>,
    #[serde(rename = "orderID", deserialize_with = "text")]
    pub order_i_d: Option<String>,
    #[serde(rename = "id", deserialize_with = "text")]
    pub id: Option<String>,
    #[serde(rename = "i", deserialize_with = "text")]
    pub i: Option<String>,
    #[serde(rename = "clientOrderId", deserialize_with = "text")]
    pub client_order_id: Option<String>,
    #[serde(rename = "clientId", deserialize_with = "text")]
    pub client_id: Option<String>,
    #[serde(rename = "fee", deserialize_with = "text")]
    pub fee: Option<String>,
    #[serde(rename = "fillFee", deserialize_with = "text")]
    pub fill_fee: Option<String>,
    #[serde(rename = "execFee", deserialize_with = "text")]
    pub exec_fee: Option<String>,
    #[serde(rename = "cumSuccessFillFee", deserialize_with = "text")]
    pub cum_success_fill_fee: Option<String>,
    #[serde(rename = "cumMatchFillFee", deserialize_with = "text")]
    pub cum_match_fill_fee: Option<String>,
    #[serde(rename = "token", deserialize_with = "text")]
    pub token: Option<String>,
    #[serde(rename = "feeToken", deserialize_with = "text")]
    pub fee_token: Option<String>,
    #[serde(rename = "feeAsset", deserialize_with = "text")]
    pub fee_asset: Option<String>,
    #[serde(rename = "feeCurrency", deserialize_with = "text")]
    pub fee_currency: Option<String>,
    #[serde(rename = "settleAssetId", deserialize_with = "text")]
    pub settle_asset_id: Option<String>,
    #[serde(rename = "tradeId", deserialize_with = "text")]
    pub trade_id: Option<String>,
    #[serde(rename = "tradeID", deserialize_with = "text")]
    pub trade_i_d: Option<String>,
    #[serde(rename = "fillId", deserialize_with = "text")]
    pub fill_id: Option<String>,
    #[serde(rename = "execId", deserialize_with = "text")]
    pub exec_id: Option<String>,
}
