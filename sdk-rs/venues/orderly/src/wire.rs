use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;
use std::borrow::Cow;

pub fn number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum N<'a> {
        Number(f64),
        Text(#[serde(borrow)] Cow<'a, str>),
        Null,
    }
    let n = match N::deserialize(d)? {
        N::Number(n) => n,
        N::Text(s) => s.parse().map_err(serde::de::Error::custom)?,
        N::Null => return Err(serde::de::Error::custom("null number")),
    };
    if n.is_finite() {
        Ok(n)
    } else {
        Err(serde::de::Error::custom("nonfinite number"))
    }
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, borrow)]
    pub event: Cow<'a, str>,
    #[serde(default, borrow)]
    pub topic: Cow<'a, str>,
    #[serde(default, borrow)]
    pub id: Cow<'a, str>,
    pub success: Option<bool>,
    #[serde(default)]
    pub ts: u64,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}

#[derive(Deserialize)]
pub struct Rest<'a> {
    pub success: bool,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
pub fn checked(response: safe_cex_engine::Response<'_>) -> Result<Rest<'_>, String> {
    let r: Rest<'_> = serde_json::from_str(response.body).map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status) || !r.success {
        return Err(format!("Orderly HTTP {}: {}", response.status, r.message));
    }
    Ok(r)
}

#[derive(Deserialize)]
pub struct MarketRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub base_tick: f64,
    #[serde(deserialize_with = "number")]
    pub quote_tick: f64,
    #[serde(deserialize_with = "number")]
    pub base_min: f64,
    #[serde(deserialize_with = "number")]
    pub base_max: f64,
    #[serde(default, deserialize_with = "number")]
    pub base_imr: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_leverage: f64,
}

#[derive(Deserialize)]
pub struct TickerRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "24h_open", deserialize_with = "number")]
    pub open: f64,
    #[serde(rename = "24h_close", deserialize_with = "number")]
    pub close: f64,
    #[serde(default, deserialize_with = "number")]
    pub index_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(default, deserialize_with = "number")]
    pub last_funding_rate: f64,
    #[serde(default, deserialize_with = "number")]
    pub next_funding_time: f64,
    #[serde(default, deserialize_with = "number")]
    pub open_interest: f64,
    #[serde(rename = "24h_volume", default, deserialize_with = "number")]
    pub volume: f64,
    #[serde(rename = "24h_amount", default, deserialize_with = "number")]
    pub amount: f64,
}

#[derive(Deserialize)]
pub struct LiveTicker<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    pub close: f64,
    #[serde(default, deserialize_with = "number")]
    pub volume: f64,
    #[serde(default, deserialize_with = "number")]
    pub amount: f64,
    #[serde(default, deserialize_with = "number")]
    pub bid: f64,
    #[serde(default, deserialize_with = "number")]
    pub ask: f64,
    #[serde(default, deserialize_with = "number")]
    pub price: f64,
    #[serde(rename = "fundingRate", default, deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(rename = "fundingTs", default, deserialize_with = "number")]
    pub funding_time: f64,
}

#[derive(Deserialize)]
pub struct FundingTicker<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "fundingRate", deserialize_with = "number")]
    pub funding_rate: f64,
    #[serde(rename = "fundingTs", deserialize_with = "number")]
    pub funding_time: f64,
}

#[derive(Deserialize)]
pub struct Depth {
    #[serde(rename = "prevTs")]
    pub previous: Option<u64>,
    pub symbol: Option<String>,
    #[serde(default)]
    pub ts: u64,
    #[serde(default)]
    pub timestamp: u64,
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}
impl Depth {
    pub fn valid(&self, sorted: bool) -> bool {
        [&self.bids, &self.asks].iter().all(|side| {
            side.iter()
                .all(|(p, q)| p.is_finite() && *p > 0.0 && q.is_finite() && *q >= 0.0)
        }) && (!sorted
            || (self.bids.windows(2).all(|w| w[0].0 > w[1].0)
                && self.asks.windows(2).all(|w| w[0].0 < w[1].0)))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    #[serde(alias = "start_timestamp", deserialize_with = "number")]
    pub start_time: f64,
    #[serde(deserialize_with = "number")]
    pub open: f64,
    #[serde(deserialize_with = "number")]
    pub high: f64,
    #[serde(deserialize_with = "number")]
    pub low: f64,
    #[serde(deserialize_with = "number")]
    pub close: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}
impl Kline {
    pub fn candle(self) -> Result<safe_cex_engine::model::Candle, String> {
        if !(0.0..=253402300799000.0).contains(&self.start_time)
            || self.volume < 0.0
            || self.open <= 0.0
            || self.close <= 0.0
            || self.low <= 0.0
            || self.high < self.low
            || self.high < self.open.max(self.close)
            || self.low > self.open.min(self.close)
        {
            return Err("Orderly invalid candle".into());
        }
        Ok(safe_cex_engine::model::Candle {
            timestamp: self.start_time / 1000.0,
            open: self.open,
            high: self.high,
            low: self.low,
            close: self.close,
            volume: self.volume,
        })
    }
}

#[derive(Clone, Default, Debug)]
pub struct Id(pub String);
impl<'de> Deserialize<'de> for Id {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum I<'a> {
            Number(u64),
            Text(#[serde(borrow)] Cow<'a, str>),
        }
        let s = match I::deserialize(d)? {
            I::Number(n) => n.to_string(),
            I::Text(s) => s.into_owned(),
        };
        if s.is_empty()
            || !s.bytes().all(|b| b.is_ascii_digit())
            || (s.len() > 1 && s.starts_with('0'))
        {
            return Err(serde::de::Error::custom("invalid decimal ID"));
        }
        Ok(Self(s))
    }
}
#[derive(Clone, Copy, Deserialize, Default)]
#[serde(transparent)]
pub struct N(#[serde(deserialize_with = "number")] pub f64);

#[derive(Deserialize)]
pub struct Info {
    #[serde(deserialize_with = "number")]
    pub max_leverage: f64,
}
#[derive(Deserialize)]
pub struct ModeRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub default_margin_mode: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct PositionRow<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(alias = "positionQty", deserialize_with = "number")]
    pub position_qty: f64,
    #[serde(alias = "averageOpenPrice", default, deserialize_with = "number")]
    pub average_open_price: f64,
    #[serde(alias = "markPrice", default, deserialize_with = "number")]
    pub mark_price: f64,
    #[serde(alias = "estLiqPrice", default, deserialize_with = "number")]
    pub est_liq_price: f64,
    #[serde(alias = "marginMode", default, borrow)]
    pub margin_mode: Cow<'a, str>,
    #[serde(default, deserialize_with = "number")]
    pub leverage: f64,
    #[serde(default, deserialize_with = "number")]
    pub imr: f64,
    #[serde(default, deserialize_with = "number")]
    pub margin: f64,
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub timestamp: Option<u64>,
}
#[derive(Deserialize)]
pub struct PositionPush<'a> {
    #[serde(borrow)]
    pub positions: Vec<PositionRow<'a>>,
}
#[derive(Deserialize)]
pub struct PositionMeta {
    #[serde(deserialize_with = "number")]
    pub total_collateral_value: f64,
    #[serde(deserialize_with = "number")]
    pub free_collateral: f64,
}
#[derive(Deserialize)]
pub struct Meta {
    pub total: Option<u64>,
    pub current_page: Option<u32>,
    pub records_per_page: Option<u32>,
}
#[derive(Deserialize)]
pub struct Page {
    pub meta: Option<Meta>,
}

#[derive(Deserialize)]
pub struct OrderRow<'a> {
    #[serde(alias = "orderId", default)]
    pub order_id: Id,
    #[serde(alias = "algoOrderId", default)]
    pub algo_order_id: Id,
    #[serde(alias = "rootAlgoOrderId", default)]
    pub root_algo_order_id: Id,
    #[serde(default, borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "type", alias = "order_type", default, borrow)]
    pub kind: Cow<'a, str>,
    #[serde(alias = "algoType", default, borrow)]
    pub algo_type: Cow<'a, str>,
    #[serde(default, borrow)]
    pub side: Cow<'a, str>,
    #[serde(default, borrow)]
    pub status: Cow<'a, str>,
    #[serde(alias = "algoStatus", default, borrow)]
    pub algo_status: Cow<'a, str>,
    #[serde(
        alias = "rootAlgoStatus",
        alias = "root_algo_order_status",
        default,
        borrow
    )]
    pub root_algo_status: Cow<'a, str>,
    #[serde(alias = "order_price", default, deserialize_with = "number")]
    pub price: f64,
    #[serde(alias = "order_quantity", default, deserialize_with = "number")]
    pub quantity: f64,
    #[serde(alias = "totalExecutedQuantity", default)]
    pub total_executed_quantity: Option<N>,
    #[serde(default)]
    pub executed: Option<N>,
    #[serde(alias = "executedQuantity", default)]
    pub executed_quantity: Option<N>,
    #[serde(alias = "executedPrice", default)]
    pub executed_price: Option<N>,
    #[serde(alias = "averageExecutedPrice", alias = "avgExecutedPrice", default)]
    pub average_executed_price: Option<N>,
    #[serde(alias = "reduceOnly", default)]
    pub reduce_only: Option<bool>,
    #[serde(alias = "clientOrderId", default, borrow)]
    pub client_order_id: Option<Cow<'a, str>>,
    #[serde(alias = "marginMode", default, borrow)]
    pub margin_mode: Cow<'a, str>,
    #[serde(alias = "timeInForce", default, borrow)]
    pub time_in_force: Cow<'a, str>,
    #[serde(alias = "triggerPrice", default)]
    pub trigger_price: Option<N>,
    #[serde(alias = "triggerPriceType", default, borrow)]
    pub trigger_price_type: Cow<'a, str>,
    #[serde(alias = "childOrders", default, borrow)]
    pub child_orders: Vec<OrderRow<'a>>,
    #[serde(alias = "tradeId", default)]
    pub trade_id: Id,
    #[serde(default)]
    pub match_id: Id,
    #[serde(default)]
    pub seq: Option<u64>,
    #[serde(default)]
    pub timestamp: Option<u64>,
}
#[derive(Deserialize)]
pub struct RestProof<'a> {
    pub order_id: Id,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(borrow)]
    pub side: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    pub price: f64,
    pub quantity: f64,
    pub visible_quantity: f64,
    pub total_executed_quantity: f64,
    pub executed_quantity: Option<f64>,
    pub executed: Option<f64>,
    pub reduce_only: bool,
    #[serde(borrow)]
    pub margin_mode: Cow<'a, str>,
    #[serde(default, borrow)]
    pub time_in_force: Cow<'a, str>,
    pub trigger_price: Option<N>,
    pub algo_order_id: Option<Id>,
    pub root_algo_order_id: Option<Id>,
    #[serde(default, borrow)]
    pub algo_type: Cow<'a, str>,
}
