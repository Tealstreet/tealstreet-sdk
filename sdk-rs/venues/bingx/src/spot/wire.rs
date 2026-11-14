//! Spot v3 codecs. Native integers stay exact; malformed financial rows fail closed.
use safe_cex_engine::{
    Response, VenueError,
    de::{Number, number},
    model::*,
};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::borrow::Cow;

pub fn symbol(id: &str) -> String {
    id.replace('-', "")
}
pub fn stream_symbol<'a>(id: &str, scratch: &'a mut String) -> &'a str {
    scratch.clear();
    scratch.extend(id.split('-'));
    scratch
}
pub fn invalid(message: impl Into<String>) -> VenueError {
    VenueError::new(message)
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Id {
    Text(String),
    Integer(u64),
    Signed(i64),
}
impl Id {
    pub fn text(&self) -> String {
        match self {
            Self::Text(v) => v.clone(),
            Self::Integer(v) => v.to_string(),
            Self::Signed(v) => v.to_string(),
        }
    }
    pub fn valid(&self) -> bool {
        let s = self.text();
        !s.is_empty() && !s.starts_with('0') && s.bytes().all(|b| b.is_ascii_digit())
    }
}
#[derive(Deserialize)]
pub struct Envelope<T> {
    pub code: i64,
    #[serde(default)]
    pub msg: String,
    pub data: Option<T>,
}
pub fn data<T: serde::de::DeserializeOwned>(response: Response<'_>) -> Result<T, VenueError> {
    if !(200..300).contains(&response.status) {
        return Err(invalid(format!("BingX Spot HTTP {}", response.status)));
    }
    let e: Envelope<T> =
        serde_json::from_str(response.body).map_err(|_| invalid("Invalid BingX Spot response"))?;
    if e.code != 0 {
        return Err(invalid(format!("BingX Spot {}: {}", e.code, e.msg)));
    }
    e.data
        .ok_or_else(|| invalid("Missing BingX Spot response data"))
}
#[derive(Deserialize)]
pub struct Symbols {
    pub symbols: Vec<SpotMarket>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotMarket {
    pub symbol: String,
    #[serde(deserialize_with = "number")]
    pub status: f64,
    #[serde(deserialize_with = "number")]
    pub tick_size: f64,
    #[serde(deserialize_with = "number")]
    pub step_size: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_qty: f64,
    #[serde(default, deserialize_with = "number")]
    pub max_qty: f64,
    #[serde(deserialize_with = "number")]
    pub min_notional: f64,
    #[serde(deserialize_with = "number")]
    pub max_notional: f64,
    pub api_state_buy: Option<bool>,
    pub api_state_sell: Option<bool>,
}
impl SpotMarket {
    pub fn map(&self) -> Result<Market, VenueError> {
        let (base, quote) = self
            .symbol
            .split_once('-')
            .filter(|(b, q)| !b.is_empty() && !q.is_empty())
            .ok_or_else(|| invalid("Invalid Spot symbol"))?;
        if self.tick_size <= 0.
            || self.step_size <= 0.
            || self.min_notional < 0.
            || self.max_notional <= 0.
        {
            return Err(invalid("Invalid Spot trading rules"));
        }
        Ok(Market {
            id: self.symbol.clone(),
            symbol: symbol(&self.symbol),
            pretty_symbol: None,
            base: base.into(),
            quote: quote.into(),
            active: self.status == 1.,
            precision: Precision {
                amount: self.step_size,
                price: self.tick_size,
            },
            limits: Limits {
                amount: MinMax {
                    min: self.min_qty.max(0.),
                    max: if self.max_qty > 0. {
                        self.max_qty
                    } else {
                        f64::MAX
                    },
                },
                leverage: MinMax { min: 1., max: 1. },
                price: None,
            },
            contract_size: None,
            is_expiring: Some(false),
            is_inverse: Some(false),
            is_spot: Some(true),
            is_perpetual: Some(false),
            is_read_only: Some(
                self.api_state_buy == Some(false) && self.api_state_sell == Some(false),
            ),
            taker_fee: None,
            maker_fee: None,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily {
    pub symbol: String,
    #[serde(deserialize_with = "number")]
    pub last_price: f64,
    #[serde(deserialize_with = "number")]
    pub bid_price: f64,
    #[serde(deserialize_with = "number")]
    pub ask_price: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
    #[serde(deserialize_with = "number")]
    pub quote_volume: f64,
    pub price_change_percent: String,
}
impl Daily {
    pub fn map(self) -> Result<Ticker, VenueError> {
        let percentage = self
            .price_change_percent
            .trim_end_matches('%')
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite())
            .ok_or_else(|| invalid("Invalid Spot ticker percentage"))?;
        if self.last_price <= 0.
            || self.bid_price < 0.
            || self.ask_price < 0.
            || self.volume < 0.
            || self.quote_volume < 0.
        {
            return Err(invalid("Invalid Spot ticker"));
        }
        Ok(Ticker {
            id: self.symbol.clone(),
            symbol: symbol(&self.symbol),
            last: self.last_price,
            bid: self.bid_price,
            ask: self.ask_price,
            percentage,
            volume: self.volume,
            quote_volume: self.quote_volume,
            ..Ticker::default()
        })
    }
}
#[derive(Deserialize)]
pub struct Balances {
    pub balances: Vec<Wallet>,
}
#[derive(Deserialize)]
pub struct Wallet {
    pub asset: String,
    #[serde(deserialize_with = "number")]
    pub free: f64,
    #[serde(deserialize_with = "number")]
    pub locked: f64,
}
impl Wallet {
    pub fn map(self) -> Result<CurrencyBalance, VenueError> {
        if self.asset.is_empty()
            || self.free < 0.
            || self.locked < 0.
            || !(self.free + self.locked).is_finite()
        {
            return Err(invalid("Invalid Spot balance"));
        }
        Ok(CurrencyBalance {
            currency: self.asset,
            balance: Balance {
                free: self.free,
                used: self.locked,
                total: self.free + self.locked,
                upnl: 0.,
            },
        })
    }
}
#[derive(Deserialize)]
pub struct Orders {
    pub orders: Vec<SpotOrder>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SpotOrder {
    pub symbol: String,
    pub order_id: Id,
    #[serde(rename = "type")]
    pub kind: String,
    pub side: String,
    pub status: String,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub orig_qty: f64,
    #[serde(deserialize_with = "number")]
    pub executed_qty: f64,
    #[serde(default, deserialize_with = "number")]
    pub time: f64,
    #[serde(default, deserialize_with = "number")]
    pub update_time: f64,
    #[serde(rename = "clientOrderID", alias = "clientOrderId")]
    pub client_order_id: Option<String>,
    pub time_in_force: Option<String>,
    #[serde(default, alias = "StopPrice", deserialize_with = "number")]
    pub stop_price: f64,
}
pub fn side(s: &str) -> Result<OrderSide, VenueError> {
    match s {
        "BUY" => Ok(OrderSide::Buy),
        "SELL" => Ok(OrderSide::Sell),
        _ => Err(invalid("Unknown Spot order side")),
    }
}
pub fn status(s: &str) -> Result<OrderStatus, VenueError> {
    match s {
        "NEW" | "PENDING" | "PARTIALLY_FILLED" => Ok(OrderStatus::Open),
        "FILLED" => Ok(OrderStatus::Closed),
        "CANCELED" | "CANCELLED" | "EXPIRED" | "REJECTED" => Ok(OrderStatus::Canceled),
        _ => Err(invalid("Unknown Spot order status")),
    }
}
pub fn kind(s: &str) -> Result<OrderType, VenueError> {
    match s {
        "LIMIT" => Ok(OrderType::Limit),
        "MARKET" => Ok(OrderType::Market),
        "TAKE_STOP_LIMIT" | "TAKE_STOP_MARKET" | "TRIGGER_LIMIT" | "TRIGGER_MARKET" => {
            Ok(OrderType::Conditional)
        }
        _ => Err(invalid("Unknown Spot order type")),
    }
}
impl SpotOrder {
    pub fn map(self) -> Result<Order, VenueError> {
        if !self.order_id.valid()
            || self.orig_qty <= 0.
            || self.executed_qty < 0.
            || self.executed_qty > self.orig_qty
            || self.price < 0.
        {
            return Err(invalid("Invalid Spot order"));
        }
        Ok(Order {
            id: self.order_id.text(),
            symbol: symbol(&self.symbol),
            kind: kind(&self.kind)?,
            side: side(&self.side)?,
            status: status(&self.status)?,
            amount: self.orig_qty,
            filled: self.executed_qty,
            remaining: self.orig_qty - self.executed_qty,
            price: if self.stop_price > 0. {
                self.stop_price
            } else {
                self.price
            },
            client_order_id: self.client_order_id,
            last_update: self.update_time.max(self.time),
            time_in_force: match self.time_in_force.as_deref() {
                Some("GTC") => Some(TimeInForce::GoodTillCancel),
                Some("IOC") => Some(TimeInForce::ImmediateOrCancel),
                Some("FOK") => Some(TimeInForce::FillOrKill),
                Some("PostOnly") => Some(TimeInForce::PostOnly),
                _ => None,
            },
            parent_id: None,
            reduce_only: false,
            close: None,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
}
#[derive(Deserialize)]
pub struct Fills {
    pub fills: Vec<Trade>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Trade {
    pub symbol: String,
    pub id: Id,
    pub order_id: Id,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub qty: f64,
    #[serde(deserialize_with = "number")]
    pub commission: f64,
    pub commission_asset: String,
    #[serde(deserialize_with = "number")]
    pub time: f64,
    pub is_buyer: bool,
    pub is_maker: bool,
}
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Execution {
    pub id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub qty: f64,
    pub timestamp: f64,
    pub commission: f64,
    pub commission_asset: String,
    pub maker: bool,
}
impl Trade {
    pub fn map(self) -> Result<Execution, VenueError> {
        if !self.id.valid()
            || !self.order_id.valid()
            || self.price <= 0.
            || self.qty <= 0.
            || self.time < 0.
        {
            return Err(invalid("Invalid Spot fill"));
        }
        Ok(Execution {
            id: self.id.text(),
            order_id: self.order_id.text(),
            symbol: symbol(&self.symbol),
            side: if self.is_buyer {
                OrderSide::Buy
            } else {
                OrderSide::Sell
            },
            price: self.price,
            qty: self.qty,
            timestamp: (self.time / 1000.).floor(),
            commission: self.commission.abs(),
            commission_asset: self.commission_asset,
            maker: self.is_maker,
        })
    }
}
#[derive(Deserialize)]
pub struct Depth {
    pub bids: Vec<(Number, Number)>,
    pub asks: Vec<(Number, Number)>,
}
impl Depth {
    pub fn valid(&self) -> bool {
        self.bids
            .iter()
            .chain(&self.asks)
            .all(|(p, q)| p.0 > 0. && q.0 >= 0.)
    }
}
pub fn candle(row: Vec<Number>) -> Result<Candle, VenueError> {
    if row.len() < 6 {
        return Err(invalid("Short Spot candle"));
    }
    let v: Vec<f64> = row.into_iter().map(|n| n.0).collect();
    if v[0] < 0. || v[1..5].iter().any(|v| *v <= 0.) || v[5] < 0. || v[2] < v[3] {
        return Err(invalid("Invalid Spot candle"));
    }
    Ok(Candle {
        timestamp: v[0] / 1000.,
        open: v[1],
        high: v[2],
        low: v[3],
        close: v[4],
        volume: v[5],
    })
}
pub fn interval(i: Interval, ws: bool) -> Option<&'static str> {
    Some(match i {
        Interval::M1 => {
            if ws {
                "1min"
            } else {
                "1m"
            }
        }
        Interval::M3 => {
            if ws {
                "3min"
            } else {
                "3m"
            }
        }
        Interval::M5 => {
            if ws {
                "5min"
            } else {
                "5m"
            }
        }
        Interval::M15 => {
            if ws {
                "15min"
            } else {
                "15m"
            }
        }
        Interval::M30 => {
            if ws {
                "30min"
            } else {
                "30m"
            }
        }
        Interval::H1 => "1h",
        Interval::H2 => "2h",
        Interval::H4 => "4h",
        Interval::H6 => "6h",
        Interval::H8 => "8h",
        Interval::H12 => "12h",
        Interval::D1 => "1d",
        Interval::D3 => "3d",
        Interval::W1 => "1w",
        Interval::Mo1 => "1M",
        _ => return None,
    })
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct GeneralMessage<'a> {
    #[serde(default)]
    pub code: i64,
    #[serde(default, borrow)]
    pub data_type: Cow<'a, str>,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
    #[serde(default, borrow)]
    pub e: Cow<'a, str>,
}
pub struct Message<'a> {
    pub code: i64,
    pub data_type: Cow<'a, str>,
    pub data: Option<&'a RawValue>,
    pub e: Cow<'a, str>,
    pub ticker: Option<LiveTicker<'a>>,
    pub depth: Option<Depth>,
}
impl<'a> From<GeneralMessage<'a>> for Message<'a> {
    fn from(message: GeneralMessage<'a>) -> Self {
        Self {
            code: message.code,
            data_type: message.data_type,
            data: message.data,
            e: message.e,
            ticker: None,
            depth: None,
        }
    }
}
impl<'a> Message<'a> {
    #[inline(always)]
    pub fn decode(text: &'a str, public: bool) -> serde_json::Result<Self> {
        if public {
            // Like the swap decoder, retain the general path for reordered fields
            // and typed row errors; private notifications always use that path.
            compact_public_message(text).map_or_else(
                || serde_json::from_str::<GeneralMessage<'a>>(text).map(Into::into),
                Ok,
            )
        } else {
            serde_json::from_str::<GeneralMessage<'a>>(text).map(Into::into)
        }
    }
}
// The common compact public envelope needs no RawValue scan before its row.
// Other field orders, escapes and optional fields retain the general decoder.
fn compact_public_message(text: &str) -> Option<Message<'_>> {
    let rest = text
        .strip_prefix(r#"{"code":0,"dataType":""#)
        .or_else(|| text.strip_prefix(r#"{"dataType":""#))?;
    let end = rest.find('"')?;
    let channel = &rest[..end];
    if channel.bytes().any(|byte| byte < 0x20 || byte == b'\\') {
        return None;
    }
    let payload = rest[end..]
        .strip_prefix(r#"","data":"#)?
        .strip_suffix('}')?;
    let mut message = Message {
        code: 0,
        data_type: Cow::Borrowed(channel),
        data: None,
        e: Cow::Borrowed(""),
        ticker: None,
        depth: None,
    };
    if channel.ends_with("@ticker") {
        message.ticker = Some(serde_json::from_str(payload).ok()?);
    } else if channel.ends_with("@depth50") {
        message.depth = Some(serde_json::from_str(payload).ok()?);
    } else {
        return None;
    }
    Some(message)
}
#[derive(Deserialize)]
pub struct LiveTicker<'a> {
    #[serde(borrow)]
    pub s: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(rename = "P", deserialize_with = "number")]
    pub percentage: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
    #[serde(deserialize_with = "number")]
    pub q: f64,
    #[serde(rename = "B", deserialize_with = "number")]
    pub bid: f64,
    #[serde(rename = "A", deserialize_with = "number")]
    pub ask: f64,
}
#[derive(Deserialize)]
pub struct LiveCandle {
    pub s: String,
    #[serde(rename = "K")]
    pub k: Bar,
}
#[derive(Deserialize)]
pub struct Bar {
    #[serde(deserialize_with = "number")]
    pub t: f64,
    #[serde(deserialize_with = "number")]
    pub o: f64,
    #[serde(deserialize_with = "number")]
    pub h: f64,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(deserialize_with = "number")]
    pub c: f64,
    #[serde(deserialize_with = "number")]
    pub v: f64,
}
#[derive(Deserialize)]
pub struct Report {
    pub s: String,
    #[serde(rename = "i")]
    pub id: Id,
    #[serde(rename = "S")]
    pub side: String,
    #[serde(rename = "o")]
    pub kind: String,
    #[serde(rename = "X")]
    pub status: String,
    #[serde(deserialize_with = "number")]
    pub q: f64,
    #[serde(deserialize_with = "number")]
    pub p: f64,
    #[serde(deserialize_with = "number")]
    pub z: f64,
    #[serde(deserialize_with = "number")]
    pub l: f64,
    #[serde(rename = "L", deserialize_with = "number")]
    pub last_price: f64,
    #[serde(default)]
    pub t: Option<Id>,
    #[serde(rename = "T", deserialize_with = "number")]
    pub time: f64,
    #[serde(default, rename = "C", alias = "c")]
    pub c: Option<String>,
    #[serde(default, deserialize_with = "number")]
    pub n: f64,
}
impl Report {
    pub fn order(&self) -> Result<Order, VenueError> {
        if !self.id.valid()
            || self.q <= 0.
            || self.z < 0.
            || self.z > self.q
            || self.l < 0.
            || self.l > self.z
            || self.p < 0.
        {
            return Err(invalid("Invalid Spot execution report"));
        }
        Ok(Order {
            id: self.id.text(),
            symbol: symbol(&self.s),
            side: side(&self.side)?,
            kind: kind(&self.kind)?,
            status: status(&self.status)?,
            amount: self.q,
            filled: self.z,
            remaining: self.q - self.z,
            price: self.p,
            last_update: self.time,
            client_order_id: self.c.clone(),
            time_in_force: None,
            parent_id: None,
            reduce_only: false,
            close: None,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stream_symbol_reuses_storage_without_changing_normalization() {
        let mut scratch = String::with_capacity(64);
        let allocation = scratch.as_ptr();
        for id in ["BTC-USDT", "ETH--USDT", "BTCUSDT", "", "---", "é-USD"] {
            assert_eq!(stream_symbol(id, &mut scratch), symbol(id));
            assert_eq!(scratch.as_ptr(), allocation);
        }
    }

    #[test]
    fn stream_identifiers_borrow_the_frame_instead_of_allocating_per_tick() {
        let frame = r#"{"code":0,"data":{"s":"BTC-USDT","c":"42","P":"1","v":"2","q":"84","B":"41","A":"43"},"dataType":"BTC-USDT@ticker"}"#;
        let message: GeneralMessage<'_> = serde_json::from_str(frame).unwrap();
        assert!(
            frame
                .as_bytes()
                .as_ptr_range()
                .contains(&message.data_type.as_ptr())
        );
        let ticker: LiveTicker = serde_json::from_str(message.data.unwrap().get()).unwrap();
        assert!(frame.as_bytes().as_ptr_range().contains(&ticker.s.as_ptr()));
    }

    #[test]
    fn public_rows_decode_in_the_envelope_pass_and_escaped_identifiers_still_work() {
        let frame = r#"{"dataType":"BTC-USDT@ticker","data":{"s":"BTC-USDT","c":"42","P":"1","v":"2","q":"84","B":"41","A":"43"}}"#;
        let message = Message::decode(frame, true).unwrap();
        assert!(
            message.data.is_none(),
            "the payload must not be scanned twice"
        );
        let ticker = message.ticker.unwrap();
        assert_eq!((ticker.c, ticker.bid, ticker.ask), (42., 41., 43.));
        assert!(frame.as_bytes().as_ptr_range().contains(&ticker.s.as_ptr()));

        let depth = Message::decode(
            r#"{"dataType":"BTC-USDT@depth50","data":{"bids":[["41","2"]],"asks":[[43,"3"]]}}"#,
            true,
        )
        .unwrap();
        assert!(depth.data.is_none());
        assert_eq!(depth.depth.unwrap().bids[0].1.0, 2.);

        let escaped = frame.replace("BTC-USDT", r"BTC\u002dUSDT");
        let message = Message::decode(&escaped, true).unwrap();
        assert_eq!(message.data_type, "BTC-USDT@ticker");
        assert_eq!(projection(message)[3][0], "BTC-USDT");
    }

    fn projection(message: Message<'_>) -> serde_json::Value {
        let raw = message.data.map(RawValue::get);
        let ticker = message
            .ticker
            .or_else(|| raw.and_then(|data| serde_json::from_str::<LiveTicker<'_>>(data).ok()))
            .map(|ticker| {
                serde_json::json!([
                    ticker.s,
                    ticker.c,
                    ticker.percentage,
                    ticker.v,
                    ticker.q,
                    ticker.bid,
                    ticker.ask
                ])
            });
        let depth = message
            .depth
            .or_else(|| raw.and_then(|data| serde_json::from_str::<Depth>(data).ok()))
            .map(|depth| {
                let rows = |side: Vec<(Number, Number)>| {
                    side.into_iter()
                        .map(|(p, q)| (p.0, q.0))
                        .collect::<Vec<_>>()
                };
                serde_json::json!([rows(depth.bids), rows(depth.asks)])
            });
        serde_json::json!([message.code, message.data_type, message.e, ticker, depth])
    }

    #[test]
    fn optimized_public_decode_preserves_general_decoder_results_and_refusals() {
        let ticker = r#"{"s":"BTC-USDT","c":"42","P":"1","v":"2","q":"84","B":"41","A":"43"}"#;
        let depth = r#"{"bids":[["41","2"],["41","0"]],"asks":[[43,"3"]]}"#;
        for (channel, payload) in [("BTC-USDT@ticker", ticker), ("BTC-USDT@depth50", depth)] {
            for frame in [
                format!(r#"{{"dataType":"{channel}","data":{payload}}}"#),
                format!(r#"{{"data":{payload},"dataType":"{channel}"}}"#),
                format!(r#"{{"dataType":"{channel}","data":{payload},"code":9}}"#),
                format!(r#"{{"dataType":"{channel}","data":{payload},"e":"ACCOUNT_UPDATE"}}"#),
                format!(
                    r#"{{"unknown":{{"nested":[1,2]}},"dataType":"{channel}","data":{payload}}}"#
                ),
                format!(r#"{{"dataType":"{channel}","data":null}}"#),
                format!(
                    r#"{{"dataType":"{channel}","data":{{"s":false,"bids":[[true,2]],"asks":[]}}}}"#
                ),
                format!(r#"{{"dataType":"{channel}","data":{payload},"data":null}}"#),
                format!(r#"{{"dataType":"{channel}","dataType":"other","data":{payload}}}"#),
                format!(r#"{{"code":null,"dataType":"{channel}","data":{payload}}}"#),
                format!(r#"{{"e":null,"dataType":"{channel}","data":{payload}}}"#),
            ] {
                let general = serde_json::from_str::<GeneralMessage<'_>>(&frame).map(Message::from);
                let optimized = Message::decode(&frame, true);
                assert_eq!(general.is_ok(), optimized.is_ok(), "{frame}");
                if let (Ok(general), Ok(optimized)) = (general, optimized) {
                    assert_eq!(projection(general), projection(optimized), "{frame}");
                }
            }
        }
    }

    #[test]
    fn compact_envelopes_keep_json_validation_and_general_decoder_semantics() {
        let payload = r#"{"s":"BTC-USDT","c":"42","P":"1","v":"2","q":"84","B":"41","A":"43"}"#;
        let base = format!(r#"{{"code":0,"dataType":"BTC-USDT@ticker","data":{payload}}}"#);
        let check = |text: &str| {
            let general = serde_json::from_str::<GeneralMessage<'_>>(text).map(Message::from);
            let optimized = Message::decode(text, true);
            assert_eq!(general.is_ok(), optimized.is_ok(), "{text:?}");
            if let (Ok(general), Ok(optimized)) = (general, optimized) {
                assert_eq!(projection(general), projection(optimized), "{text:?}");
            }
        };
        assert!(compact_public_message(&base).is_some());
        for end in 0..=base.len() {
            check(&base[..end]);
        }
        for byte in 0u8..=127 {
            let text = base.replace(
                "BTC-USDT@ticker",
                &format!("BTC{}USDT@ticker", char::from(byte)),
            );
            check(&text);
        }
        for text in [
            format!(" {base}\n"),
            base.replace("BTC-USDT", r"BTC\u002dUSDT"),
            base.replace("BTC-USDT", "é-USD"),
            base.replace(r#""code":0"#, r#""code":-0"#),
            base.replace(r#""code":0"#, r#""code":1"#),
            base.replace(r#""code":0"#, r#""code":"0""#),
            base.replace(r#""code":0"#, r#""code":0,"code":0"#),
            base.replace(r#""dataType":"#, r#""e":"ACCOUNT_UPDATE","dataType":"#),
            base.replace(r#""data":"#, r#""extra":{"text":"data"},"data":"#),
            format!("{},\"e\":\"ACCOUNT_UPDATE\"}}", &base[..base.len() - 1]),
            format!("{},\"data\":null}}", &base[..base.len() - 1]),
            format!("{base}}}"),
            format!("{base}null"),
        ] {
            check(&text);
        }
    }

    #[test]
    fn private_notifications_keep_raw_payloads_and_retirement_events() {
        for frame in [
            r#"{"dataType":"ACCOUNT_UPDATE","data":{}}"#,
            r#"{"e":"ACCOUNT_UPDATE","data":[]}"#,
            r#"{"e":"listenKeyExpired"}"#,
            r#"{"dataType":"spot.executionReport","data":{"s":"BTC-USDT","i":"9007199254740993"}}"#,
            r#"{"dataType":"BTC-USDT@depth50","e":"ACCOUNT_UPDATE","data":{"bids":[],"asks":[]}}"#,
        ] {
            let expected: GeneralMessage<'_> = serde_json::from_str(frame).unwrap();
            let actual = Message::decode(frame, false).unwrap();
            assert!(actual.ticker.is_none() && actual.depth.is_none());
            assert_eq!(actual.data_type, expected.data_type);
            assert_eq!(actual.e, expected.e);
            assert_eq!(
                actual.data.map(RawValue::get),
                expected.data.map(RawValue::get)
            );
        }
    }
}
