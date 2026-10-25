//! Bybit V5 wire records. Each struct names only the fields we read; string
//! fields borrow from the frame unless they contain escapes. Bybit sends
//! numbers as decimal strings, often empty when not applicable.
use serde::Deserialize;
use std::borrow::Cow;

pub type Str<'a> = Cow<'a, str>;
type Opt<'a> = Option<Str<'a>>;

// Cow nested inside Option/Vec does not inherit serde's direct-field borrowing.
#[derive(Deserialize)]
struct BorrowedStr<'a>(#[serde(borrow)] Cow<'a, str>);

fn borrow_optional_str<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Opt<'de>, D::Error> {
    Option::<BorrowedStr<'de>>::deserialize(deserializer).map(|value| value.map(|value| value.0))
}

/// A decimal string; empty, malformed or non-finite is NaN.
pub fn float(text: &str) -> f64 {
    safe_cex_engine::num::parse_decimal(text).unwrap_or(f64::NAN)
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
    pub _ret_code: Option<i64>,
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
    pub contract_type: Str<'a>,
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
    #[serde(borrow, default)]
    pub symbol_type: Opt<'a>,
    #[serde(borrow, default)]
    pub market_region: Opt<'a>,
    #[serde(borrow, default)]
    pub underlying_ticker: Opt<'a>,
    #[serde(borrow, default)]
    pub options_type: Opt<'a>,
    #[serde(borrow, default)]
    pub settle_coin: Opt<'a>,
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
    #[serde(borrow, default)]
    pub min_price: Opt<'a>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LotSizeFilter<'a> {
    #[serde(borrow, default)]
    pub base_precision: Opt<'a>,
    #[serde(borrow, default)]
    pub max_limit_order_qty: Opt<'a>,
    #[serde(borrow, default)]
    pub max_market_order_qty: Opt<'a>,
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
    pub symbol: Cow<'a, str>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub last_price: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub bid1_price: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub ask1_price: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub mark_price: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub index_price: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub price24h_pcnt: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub change24h: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub open_interest: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub funding_rate: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub next_funding_time: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub volume24h: Opt<'a>,
    #[serde(borrow, default, deserialize_with = "borrow_optional_str")]
    pub turnover24h: Opt<'a>,
    #[serde(borrow, default)]
    pub delta: Opt<'a>,
    #[serde(borrow, default)]
    pub gamma: Opt<'a>,
    #[serde(borrow, default)]
    pub vega: Opt<'a>,
    #[serde(borrow, default)]
    pub theta: Opt<'a>,
    #[serde(borrow, default)]
    pub mark_iv: Opt<'a>,
    #[serde(borrow, default)]
    pub bid1_iv: Opt<'a>,
    #[serde(borrow, default)]
    pub ask1_iv: Opt<'a>,
    #[serde(borrow, default)]
    pub underlying_price: Opt<'a>,
}

#[derive(Deserialize)]
pub struct TickerEvent<'a> {
    #[serde(borrow)]
    pub data: TickerDelta<'a>,
}

fn optional_string<'de, D: serde::Deserializer<'de>, T, F: FnOnce(&str) -> T>(
    deserializer: D,
    map: F,
) -> Result<Option<T>, D::Error> {
    struct OptionalString<F>(F);
    impl<'de, T, F: FnOnce(&str) -> T> serde::de::Visitor<'de> for OptionalString<F> {
        type Value = Option<T>;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a string or null")
        }
        fn visit_none<E: serde::de::Error>(self) -> Result<Self::Value, E> {
            Ok(None)
        }
        fn visit_some<D: serde::Deserializer<'de>>(
            self,
            deserializer: D,
        ) -> Result<Self::Value, D::Error> {
            deserializer.deserialize_str(self)
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<Self::Value, E> {
            Ok(Some((self.0)(value)))
        }
    }
    deserializer.deserialize_option(OptionalString(map))
}

fn optional_decimal<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<f64>, D::Error> {
    optional_string(deserializer, |text| (!text.is_empty()).then(|| float(text)))
        .map(Option::flatten)
}

fn discard_optional_string<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<(), D::Error> {
    optional_string(deserializer, |_| ()).map(|_| ())
}

/// Stream deltas hold only numeric updates. Unused option fields still validate
/// their wire types and duplicate keys without retaining their string values.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerDelta<'a> {
    #[serde(borrow, default)]
    pub symbol: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub last_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub bid1_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub ask1_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub mark_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub index_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub price24h_pcnt: Option<f64>,
    /// Option frames spell quotes and change differently from derivatives.
    #[serde(default, deserialize_with = "optional_decimal")]
    pub bid_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub ask_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub change24h: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub open_interest: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub funding_rate: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub next_funding_time: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub volume24h: Option<f64>,
    #[serde(default, deserialize_with = "optional_decimal")]
    pub turnover24h: Option<f64>,
    #[serde(
        rename = "delta",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _delta: (),
    #[serde(
        rename = "gamma",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _gamma: (),
    #[serde(rename = "vega", default, deserialize_with = "discard_optional_string")]
    _vega: (),
    #[serde(
        rename = "theta",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _theta: (),
    #[serde(
        rename = "markIv",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _mark_iv: (),
    #[serde(
        rename = "bid1Iv",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _bid1_iv: (),
    #[serde(
        rename = "ask1Iv",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _ask1_iv: (),
    #[serde(
        rename = "underlyingPrice",
        default,
        deserialize_with = "discard_optional_string"
    )]
    _underlying_price: (),
}

#[derive(Default)]
pub struct BookLevels {
    pub bids: Vec<(f64, f64)>,
    pub asks: Vec<(f64, f64)>,
}

pub struct BookMessage<'a> {
    pub kind: Cow<'a, str>,
    pub symbol: Cow<'a, str>,
    pub update: u64,
}

impl BookLevels {
    /// Refill numeric scratch space; no book state changes until the whole frame validates.
    pub fn decode<'de>(&mut self, text: &'de str) -> serde_json::Result<BookMessage<'de>> {
        self.bids.clear();
        self.asks.clear();
        let mut deserializer = serde_json::Deserializer::from_str(text);
        let message = serde::de::DeserializeSeed::deserialize(BookSeed(self), &mut deserializer)?;
        deserializer.end()?;
        Ok(message)
    }
}

// Field names are identifiers, so neither visitor needs to retain owned keys.
#[derive(Deserialize)]
#[serde(field_identifier, rename_all = "lowercase")]
enum BookField {
    Type,
    Data,
    #[serde(other)]
    Other,
}

#[derive(Deserialize)]
#[serde(field_identifier)]
enum BookDataField {
    #[serde(rename = "s")]
    Symbol,
    #[serde(rename = "u")]
    Update,
    #[serde(rename = "b")]
    Bids,
    #[serde(rename = "a")]
    Asks,
    #[serde(other)]
    Other,
}

struct BookSeed<'a>(&'a mut BookLevels);

impl<'de> serde::de::DeserializeSeed<'de> for BookSeed<'_> {
    type Value = BookMessage<'de>;
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_struct("BookMessage", &["type", "data"], self)
    }
}

impl<'de> serde::de::Visitor<'de> for BookSeed<'_> {
    type Value = BookMessage<'de>;
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("a Bybit book frame")
    }
    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        use serde::de::Error;
        let (mut kind, mut data) = (None, None);
        while let Some(key) = map.next_key::<BookField>()? {
            match key {
                BookField::Type => {
                    if kind.is_some() {
                        return Err(M::Error::duplicate_field("type"));
                    }
                    kind = Some(map.next_value::<BorrowedStr<'de>>()?.0);
                }
                BookField::Data => {
                    if data.is_some() {
                        return Err(M::Error::duplicate_field("data"));
                    }
                    data = Some(map.next_value_seed(BookDataSeed(self.0))?);
                }
                BookField::Other => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        let (symbol, update) = data.ok_or_else(|| M::Error::missing_field("data"))?;
        Ok(BookMessage {
            kind: kind.ok_or_else(|| M::Error::missing_field("type"))?,
            symbol,
            update,
        })
    }
}

struct BookDataSeed<'a>(&'a mut BookLevels);

impl<'de> serde::de::DeserializeSeed<'de> for BookDataSeed<'_> {
    type Value = (Cow<'de, str>, u64);
    fn deserialize<D: serde::Deserializer<'de>>(
        self,
        deserializer: D,
    ) -> Result<Self::Value, D::Error> {
        deserializer.deserialize_struct("BookData", &["s", "b", "a", "u"], self)
    }
}

impl<'de> serde::de::Visitor<'de> for BookDataSeed<'_> {
    type Value = (Cow<'de, str>, u64);
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Bybit book data")
    }
    fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<Self::Value, M::Error> {
        use serde::de::Error;
        let (mut symbol, mut update) = (None, None);
        let (mut bids, mut asks) = (false, false);
        while let Some(key) = map.next_key::<BookDataField>()? {
            match key {
                BookDataField::Symbol => {
                    if symbol.is_some() {
                        return Err(M::Error::duplicate_field("s"));
                    }
                    symbol = Some(map.next_value::<BorrowedStr<'de>>()?.0);
                }
                BookDataField::Update => {
                    if update.is_some() {
                        return Err(M::Error::duplicate_field("u"));
                    }
                    update = Some(map.next_value()?);
                }
                BookDataField::Bids => {
                    if bids {
                        return Err(M::Error::duplicate_field("b"));
                    }
                    bids = true;
                    map.next_value_seed(LevelsSeed(&mut self.0.bids))?;
                }
                BookDataField::Asks => {
                    if asks {
                        return Err(M::Error::duplicate_field("a"));
                    }
                    asks = true;
                    map.next_value_seed(LevelsSeed(&mut self.0.asks))?;
                }
                BookDataField::Other => {
                    map.next_value::<serde::de::IgnoredAny>()?;
                }
            }
        }
        Ok((
            symbol.ok_or_else(|| M::Error::missing_field("s"))?,
            update.unwrap_or(0),
        ))
    }
}

#[derive(Deserialize)]
struct BookLevel(
    #[serde(deserialize_with = "decimal_string")] f64,
    #[serde(deserialize_with = "decimal_string")] f64,
);

fn decimal_string<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<f64, D::Error> {
    struct Decimal;
    impl serde::de::Visitor<'_> for Decimal {
        type Value = f64;
        fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            formatter.write_str("a decimal string")
        }
        fn visit_str<E: serde::de::Error>(self, value: &str) -> Result<f64, E> {
            Ok(float(value))
        }
    }
    deserializer.deserialize_str(Decimal)
}

struct LevelsSeed<'a>(&'a mut Vec<(f64, f64)>);

impl<'de> serde::de::DeserializeSeed<'de> for LevelsSeed<'_> {
    type Value = ();
    fn deserialize<D: serde::Deserializer<'de>>(self, deserializer: D) -> Result<(), D::Error> {
        deserializer.deserialize_seq(self)
    }
}

impl<'de> serde::de::Visitor<'de> for LevelsSeed<'_> {
    type Value = ();
    fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Bybit book price/quantity pairs")
    }
    fn visit_seq<S: serde::de::SeqAccess<'de>>(self, mut seq: S) -> Result<(), S::Error> {
        while let Some(BookLevel(price, amount)) = seq.next_element()? {
            self.0.push((price, amount));
        }
        Ok(())
    }
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

#[derive(Deserialize)]
pub struct TradeEvent<'a> {
    #[serde(borrow)]
    pub topic: Str<'a>,
    #[serde(borrow, default)]
    pub data: Vec<WsTrade<'a>>,
}

/// One `publicTrade` execution: time (ms), price and size.
#[derive(Deserialize)]
pub struct WsTrade<'a> {
    #[serde(rename = "T")]
    pub time: f64,
    #[serde(rename = "p", borrow)]
    pub price: Str<'a>,
    #[serde(rename = "v", borrow)]
    pub size: Str<'a>,
}

/// REST kline row: `[startTime, open, high, low, close, volume, turnover]`.
#[derive(Deserialize)]
pub struct KlineRow<'a>(#[serde(borrow)] pub Vec<Str<'a>>);

/// A control frame: auth and subscribe acknowledgements, pongs.
#[derive(Deserialize)]
pub struct OpFrame<'a> {
    #[serde(borrow, default)]
    pub req_id: Opt<'a>,
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
    pub cum_exec_value: Opt<'a>,
    #[serde(borrow, default)]
    pub time_in_force: Opt<'a>,
    #[serde(borrow, default)]
    pub order_type: Opt<'a>,
    #[serde(borrow, default)]
    pub stop_order_type: Opt<'a>,
    #[serde(borrow, default)]
    pub trigger_price: Opt<'a>,
    #[serde(default, deserialize_with = "integer")]
    pub trigger_direction: Option<i64>,
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
    pub mark_price: Opt<'a>,
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
    #[serde(borrow, default)]
    pub delta: Opt<'a>,
    #[serde(borrow, default)]
    pub gamma: Opt<'a>,
    #[serde(borrow, default)]
    pub vega: Opt<'a>,
    #[serde(borrow, default)]
    pub theta: Opt<'a>,
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
    pub account_type: Opt<'a>,
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
    pub coin: Opt<'a>,
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

/// `/v5/order/create-batch` and `/v5/order/cancel-batch`: one result per
/// request, in request order.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Batch<'a> {
    #[serde(borrow, default)]
    pub result: Option<BatchResult<'a>>,
}

#[derive(Deserialize)]
pub struct BatchResult<'a> {
    #[serde(borrow, default)]
    pub list: Vec<PlacedResult<'a>>,
}

/// `/v5/account/transaction-log` row.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TransactionRow<'a> {
    #[serde(borrow, default)]
    pub id: Opt<'a>,
    #[serde(borrow, default)]
    pub side: Opt<'a>,
    #[serde(borrow, default)]
    pub size: Opt<'a>,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn short_unsigned_integer_float_matches_standard_parser_bitwise() {
        for integer in 0..1000 {
            let text = integer.to_string();
            assert_eq!(
                float(&text).to_bits(),
                text.parse::<f64>().unwrap().to_bits()
            );
        }
        for digits in 1..=15 {
            for digit in ["0", "1", "9"] {
                let text = digit.repeat(digits);
                assert_eq!(
                    float(&text).to_bits(),
                    text.parse::<f64>().unwrap().to_bits()
                );
            }
        }
        for text in ["000000000000001", "100000000000000", "999999999999999"] {
            assert_eq!(
                float(text).to_bits(),
                text.parse::<f64>().unwrap().to_bits()
            );
        }
    }

    #[test]
    fn non_integer_float_spellings_keep_original_parser_semantics() {
        for text in [
            "",
            "-",
            "++1",
            "1x",
            "1\0",
            "１２",
            "0.5",
            ".5",
            "5.",
            "1e-9",
            "1E12",
            "+1",
            "-1",
            "-0",
            "-0.0",
            "+0",
            " 1",
            "1 ",
            "\t1\n",
            "\u{2003}1\u{2003}",
            "0000000000000001",
            "1000000000000000",
            "9007199254740991",
            "9007199254740992",
            "9007199254740993",
            "18446744073709551615",
            "NaN",
            "nan",
            "inf",
            "-inf",
            "Infinity",
        ] {
            assert_eq!(
                float(text).to_bits(),
                text.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(f64::NAN)
                    .to_bits(),
                "changed {text:?}"
            );
        }
        assert_eq!(float("-0").to_bits(), (-0.0f64).to_bits());
    }

    #[test]
    fn short_decimal_float_matches_standard_parser_bitwise() {
        let mut state = 0x5afe_ce12_u64;
        for width in 1..=16 {
            let ceiling = 10u64.pow(width);
            let mut values = vec![0, 1, ceiling - 1, ceiling / 10, ceiling / 2];
            for _ in 0..128 {
                state = state
                    .wrapping_mul(6_364_136_223_846_793_005)
                    .wrapping_add(1);
                values.push(state % ceiling);
            }
            for value in values {
                let digits = format!("{value:0width$}", width = width as usize);
                for dot in 0..=digits.len() {
                    let mut text = digits.clone();
                    text.insert(dot, '.');
                    assert_eq!(
                        float(&text).to_bits(),
                        text.parse::<f64>().unwrap().to_bits(),
                        "changed {text:?}"
                    );
                }
            }
        }
        for text in [
            ".",
            "..",
            "1.2.3",
            "1..",
            "0.000000000000001",
            "-0.000",
            "+0.0",
        ] {
            assert_eq!(
                float(text).to_bits(),
                text.trim()
                    .parse::<f64>()
                    .ok()
                    .filter(|v| v.is_finite())
                    .unwrap_or(f64::NAN)
                    .to_bits(),
                "changed {text:?}"
            );
        }
    }

    #[test]
    fn book_decoder_reuses_numeric_buffers_across_frames() {
        let text = r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["54321","1"]],"a":[["54322","2"]],"u":2}}"#;
        let mut levels = BookLevels::default();
        levels.decode(text).unwrap();
        let before = (
            levels.bids.as_ptr(),
            levels.asks.as_ptr(),
            levels.bids.capacity(),
            levels.asks.capacity(),
        );
        levels.decode(text).unwrap();
        let after = (
            levels.bids.as_ptr(),
            levels.asks.as_ptr(),
            levels.bids.capacity(),
            levels.asks.capacity(),
        );
        assert_eq!(
            before, after,
            "book parsing allocated fresh numeric buffers"
        );
    }

    #[test]
    fn book_decoder_accepts_reordered_fields_and_rejects_incomplete_or_duplicate_frames() {
        let mut levels = BookLevels::default();
        let message = levels.decode(r#"{"d\u0061ta":{"a":[["2","3"]],"u":4,"s":"BTCUSDT","b":[["1","2"]]},"type":"delta","ignored":{"x":[1,2]}}"#).unwrap();
        assert_eq!(
            (
                message.symbol.as_ref(),
                message.kind.as_ref(),
                message.update
            ),
            ("BTCUSDT", "delta", 4)
        );
        assert_eq!(levels.bids, [(1.0, 2.0)]);
        for malformed in [
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["1","2"]],"a":[["2",3]]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["1"]]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["1","2","3"]]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":null}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[],"b":[]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","a":[],"a":[]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","u":1,"u":2}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","s":"ETHUSDT"}}"#,
            r#"{"type":"delta","type":"snapshot","data":{"s":"BTCUSDT"}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT"},"data":{"s":"ETHUSDT"}}"#,
            r#"{"type":"delta","data":{}}"#,
            r#"{"data":{"s":"BTCUSDT"}}"#,
            r#"{"type":"delta"}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT"}} {}"#,
        ] {
            assert!(
                levels.decode(malformed).is_err(),
                "accepted malformed book: {malformed}"
            );
        }
        let message = levels
            .decode(r#"{"type":"snapshot","data":{"s":"BTCUSDT"}}"#)
            .unwrap();
        assert_eq!(message.update, 0);
        assert!(
            levels.bids.is_empty() && levels.asks.is_empty(),
            "old scratch data leaked into omitted sides"
        );
    }

    #[test]
    fn book_field_identifiers_decode_all_escaped_keys_and_skip_nested_unknowns() {
        let mut levels = BookLevels::default();
        let message = levels.decode(
            r#"{"unknown":{"type":null,"data":[{"s":false}]},"\u0064\u0061\u0074\u0061":{"\u0061":[["2","3"]],"unknown":[null,{"b":false,"u":"bad"}],"\u0075":0,"\u0073":"BTCUSDT","\u0062":[["1","2"]],"unknown":true},"\u0074\u0079\u0070\u0065":"delta","\u0075nknown":[{"data":{}}]}"#,
        ).unwrap();
        assert_eq!(message.kind, "delta");
        assert_eq!(message.symbol, "BTCUSDT");
        assert_eq!(message.update, 0);
        assert!(matches!(message.kind, Cow::Borrowed(_)));
        assert!(matches!(message.symbol, Cow::Borrowed(_)));
        assert_eq!(levels.bids, [(1.0, 2.0)]);
        assert_eq!(levels.asks, [(2.0, 3.0)]);

        // A failed frame may refill scratch space, but never produces a message;
        // the next frame must start with empty sides even when they are omitted.
        for invalid in [
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["3","4"]]},"unknown":{"bad":[1,]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["3","4"]]}} trailing"#,
        ] {
            assert!(levels.decode(invalid).is_err(), "{invalid}");
            let message = levels
                .decode(r#"{"type":"snapshot","data":{"s":"ETHUSDT"}}"#)
                .unwrap();
            assert_eq!(message.symbol, "ETHUSDT");
            assert_eq!(message.update, 0);
            assert!(levels.bids.is_empty() && levels.asks.is_empty());
        }
    }

    #[test]
    fn book_field_identifiers_reject_escaped_equivalent_duplicates_in_either_order() {
        let mut levels = BookLevels::default();
        for (key, escaped, value, top_level) in [
            ("type", r"\u0074ype", r#""delta""#, true),
            ("data", r"d\u0061ta", r#"{"s":"BTCUSDT"}"#, true),
            ("s", r"\u0073", r#""BTCUSDT""#, false),
            ("u", r"\u0075", "0", false),
            ("b", r"\u0062", "[]", false),
            ("a", r"\u0061", "[]", false),
        ] {
            // Also retain rejection for two escaped spellings of the same key.
            for (first, second) in [(key, escaped), (escaped, key), (escaped, escaped)] {
                let pair = format!(r#""{first}":{value},"{second}":{value}"#);
                let text = if top_level {
                    let other = if key == "type" {
                        r#""data":{"s":"BTCUSDT"}"#
                    } else {
                        r#""type":"delta""#
                    };
                    format!("{{{pair},{other}}}")
                } else {
                    let symbol = if key == "s" { "" } else { r#""s":"BTCUSDT","# };
                    format!(r#"{{"type":"delta","data":{{{symbol}{pair}}}}}"#)
                };
                assert!(levels.decode(&text).is_err(), "accepted duplicate: {text}");
            }
        }
    }

    #[test]
    fn book_field_identifiers_preserve_known_value_type_rejection() {
        let mut levels = BookLevels::default();
        for (key, top_level, invalid_values) in [
            (r"\u0074ype", true, &["null", "false", "1", "[]", "{}"][..]),
            (
                r"d\u0061ta",
                true,
                &["null", "false", "1", "[]", r#""1""#][..],
            ),
            (r"\u0073", false, &["null", "false", "1", "[]", "{}"][..]),
            (
                r"\u0075",
                false,
                &["null", "false", r#""1""#, "[]", "{}", "1.5", "-1"][..],
            ),
            (
                r"\u0062",
                false,
                &["null", "false", r#""1""#, "1", "{}"][..],
            ),
            (
                r"\u0061",
                false,
                &["null", "false", r#""1""#, "1", "{}"][..],
            ),
        ] {
            for value in invalid_values {
                let text = if top_level {
                    let other = if key == r"\u0074ype" {
                        r#""data":{"s":"BTCUSDT"}"#
                    } else {
                        r#""type":"delta""#
                    };
                    format!(r#"{{"{key}":{value},{other}}}"#)
                } else {
                    let symbol = if key == r"\u0073" {
                        ""
                    } else {
                        r#""s":"BTCUSDT","#
                    };
                    format!(r#"{{"type":"delta","data":{{{symbol}"{key}":{value}}}}}"#)
                };
                assert!(
                    levels.decode(&text).is_err(),
                    "accepted invalid type: {text}"
                );
            }
        }
    }

    #[test]
    fn book_field_identifiers_accept_escaped_keys_and_reordered_fields() {
        let mut levels = BookLevels::default();
        let message = levels.decode(r#"{"d\u0061ta":{"\u0061":[["2","3"]],"\u0075":4,"\u0073":"BTCUSDT","\u0062":[["1","2"]]},"\u0074ype":"delta"}"#).unwrap();
        assert_eq!(
            (
                message.kind.as_ref(),
                message.symbol.as_ref(),
                message.update
            ),
            ("delta", "BTCUSDT", 4)
        );
        assert_eq!(levels.bids, [(1.0, 2.0)]);
        assert_eq!(levels.asks, [(2.0, 3.0)]);
    }

    #[test]
    fn book_field_identifiers_reject_plain_and_escaped_duplicates_in_either_order() {
        let mut levels = BookLevels::default();
        for (field, escaped, value) in [
            ("type", r"\u0074ype", r#""delta""#),
            ("data", r"d\u0061ta", r#"{"s":"BTCUSDT"}"#),
            ("s", r"\u0073", r#""BTCUSDT""#),
            ("u", r"\u0075", "2"),
            ("b", r"\u0062", "[]"),
            ("a", r"\u0061", "[]"),
        ] {
            for (first, second) in [(field, escaped), (escaped, field)] {
                let duplicate = format!(r#""{first}":{value},"{second}":{value}"#);
                let text = match field {
                    "type" => format!(r#"{{{duplicate},"data":{{"s":"BTCUSDT"}}}}"#),
                    "data" => format!(r#"{{"type":"delta",{duplicate}}}"#),
                    "s" => format!(r#"{{"type":"delta","data":{{{duplicate}}}}}"#),
                    _ => format!(r#"{{"type":"delta","data":{{"s":"BTCUSDT",{duplicate}}}}}"#),
                };
                assert!(levels.decode(&text).is_err(), "accepted duplicate: {text}");
            }
        }
    }

    #[test]
    fn book_field_identifiers_still_validate_ignored_values_and_trailing_json() {
        let mut levels = BookLevels::default();
        // Unknown fields, including duplicates after unescaping, stay ignored.
        let message = levels.decode(r#"{"extra":{"x":[1,true,null]},"ex\u0074ra":false,"type":"delta","data":{"extra":[],"ex\u0074ra":{},"s":"BTCUSDT"}}"#).unwrap();
        assert_eq!(message.update, 0);
        assert!(levels.bids.is_empty() && levels.asks.is_empty());
        for malformed in [
            r#"{"type":"delta","data":{"s":"BTCUSDT"},"extra":[1,]}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT","extra":[1,]}}"#,
            r#"{"type":"delta","data":{"s":"BTCUSDT"}} {}"#,
        ] {
            assert!(
                levels.decode(malformed).is_err(),
                "accepted malformed: {malformed}"
            );
        }
    }

    #[test]
    fn public_book_strings_borrow_unescaped_json() {
        let mut levels = BookLevels::default();
        let event = levels.decode(
            r#"{"type":"delta","data":{"s":"BTCUSDT","b":[["54321","1"]],"a":[["54322","2"]],"u":2}}"#,
        ).unwrap();
        assert_eq!(levels.bids, [(54321.0, 1.0)]);
        assert_eq!(levels.asks, [(54322.0, 2.0)]);
        assert!(
            matches!(event.symbol, Cow::Borrowed(_)),
            "book symbol allocated"
        );
        assert!(
            matches!(event.kind, Cow::Borrowed(_)),
            "book type allocated"
        );
    }

    #[test]
    fn public_ticker_decodes_numbers_and_borrows_unescaped_symbol() {
        let event: TickerEvent<'_> = serde_json::from_str(
            r#"{"data":{"symbol":"BTCUSDT","lastPrice":"54321","volume24h":"1000","turnover24h":"54321000"}}"#,
        ).unwrap();
        assert_eq!(event.data.last_price, Some(54321.0));
        assert_eq!(event.data.volume24h, Some(1000.0));
        assert!(
            matches!(event.data.symbol, Cow::Borrowed(_)),
            "ticker symbol allocated"
        );
    }

    #[test]
    fn public_book_escaped_strings_decode_into_owned_values() {
        let mut levels = BookLevels::default();
        let event = levels.decode(
            r#"{"type":"del\u0074a","data":{"s":"BTC\u0055SDT","b":[["\u00354321","1"]],"a":[],"u":2}}"#,
        ).unwrap();
        assert_eq!(event.kind, "delta");
        assert_eq!(event.symbol, "BTCUSDT");
        assert_eq!(levels.bids[0].0, 54321.0);
        assert!(matches!(event.kind, Cow::Owned(_)));
        assert!(matches!(event.symbol, Cow::Owned(_)));
        assert_eq!(levels.bids[0].1, 1.0);
    }

    #[test]
    fn public_ticker_keeps_escape_null_missing_and_empty_field_semantics() {
        let event: TickerEvent<'_> = serde_json::from_str(
            r#"{"data":{"symbol":"BTC\u0055SDT","lastPrice":"\u00354321","bid1Price":null,"volume24h":""}}"#,
        ).unwrap();
        assert_eq!(event.data.last_price, Some(54321.0));
        assert!(matches!(event.data.symbol, Cow::Owned(_)));
        assert_eq!(event.data.symbol, "BTCUSDT");
        assert!(event.data.bid1_price.is_none());
        assert!(event.data.ask1_price.is_none());
        assert!(event.data.volume24h.is_none());
    }

    #[derive(Deserialize)]
    struct LegacyTickerEvent<'a> {
        #[serde(borrow)]
        data: TickerRow<'a>,
    }

    #[test]
    fn stream_numeric_ticker_matches_legacy_conversion_on_documented_and_sparse_payloads() {
        // Bybit's ticker snapshot/delta shapes, followed by supported string edge cases.
        for text in [
            r#"{"topic":"tickers.BTCUSDT","type":"snapshot","data":{"symbol":"BTCUSDT","lastPrice":"66666.60","bid1Price":"66666.60","ask1Price":"66666.70","markPrice":"66666.60","indexPrice":"66660.19","price24hPcnt":"-0.158315","openInterest":"492373.72","fundingRate":"-0.005","nextFundingTime":"1760342400000","volume24h":"73191.3870","turnover24h":"4936790807.6521"}}"#,
            r#"{"topic":"tickers.ETHUSDT","type":"delta","data":{"symbol":"ETHUSDT","lastPrice":"3001","volume24h":"1001","turnover24h":"3004001"}}"#,
            r#"{"data":{"symbol":"BTCUSDT","lastPrice":"-0","bid1Price":"1e999","ask1Price":"-1e999","markPrice":" \t-0.0 ","indexPrice":"not-a-number","openInterest":"+0.0","fundingRate":"-0.0","volume24h":"0","turnover24h":"5e-324"}}"#,
            r#"{"data":{"symbol":"BTCUSD","lastPrice":"\u00350000","bid1Price":null,"ask1Price":"","markPrice":" 50001 ","indexPrice":"bad","price24hPcnt":"NaN","openInterest":" ","fundingRate":"-1e-4","nextFundingTime":"1760342400000","volume24h":"1000000.1250000001","turnover24h":"20.5","delta":"0.1","gamma":null,"vega":"","theta":"oops","markIv":"\u0031.2","bid1Iv":"1.1","ask1Iv":"1.3","underlyingPrice":"50000"}}"#,
            r#"{"data":{"symbol":"BTC-27MAR26-100000-C","delta":"0.5","gamma":"0.00001","vega":"2","theta":"-1","markIv":"0.6","bid1Iv":"0.59","ask1Iv":"0.61","underlyingPrice":"96000"}}"#,
        ] {
            let legacy = serde_json::from_str::<LegacyTickerEvent>(text)
                .unwrap()
                .data;
            let delta = serde_json::from_str::<TickerEvent>(text).unwrap().data;
            assert_eq!(legacy.symbol, delta.symbol);
            let expected = [
                &legacy.last_price,
                &legacy.bid1_price,
                &legacy.ask1_price,
                &legacy.mark_price,
                &legacy.index_price,
                &legacy.price24h_pcnt,
                &legacy.open_interest,
                &legacy.funding_rate,
                &legacy.next_funding_time,
                &legacy.volume24h,
                &legacy.turnover24h,
            ]
            .map(|value| present(value).map(float).map(f64::to_bits));
            let actual = [
                delta.last_price,
                delta.bid1_price,
                delta.ask1_price,
                delta.mark_price,
                delta.index_price,
                delta.price24h_pcnt,
                delta.open_interest,
                delta.funding_rate,
                delta.next_funding_time,
                delta.volume24h,
                delta.turnover24h,
            ]
            .map(|value| value.map(f64::to_bits));
            assert_eq!(actual, expected, "{text}");
        }
    }

    #[test]
    fn stream_ticker_preserves_legacy_type_duplicate_and_trailing_json_rejection() {
        for field in [
            "symbol",
            "lastPrice",
            "bid1Price",
            "ask1Price",
            "markPrice",
            "indexPrice",
            "price24hPcnt",
            "openInterest",
            "fundingRate",
            "nextFundingTime",
            "volume24h",
            "turnover24h",
            "delta",
            "gamma",
            "vega",
            "theta",
            "markIv",
            "bid1Iv",
            "ask1Iv",
            "underlyingPrice",
        ] {
            for value in ["42", "true", "[]", "{}"] {
                let text = format!(r#"{{"data":{{"{field}":{value}}}}}"#);
                assert!(
                    serde_json::from_str::<LegacyTickerEvent>(&text).is_err(),
                    "{text}"
                );
                assert!(
                    serde_json::from_str::<TickerEvent>(&text).is_err(),
                    "{text}"
                );
            }
            for value in [r#""1""#, "null"] {
                let text = format!(r#"{{"data":{{"{field}":{value},"{field}":{value}}}}}"#);
                assert!(
                    serde_json::from_str::<LegacyTickerEvent>(&text).is_err(),
                    "{text}"
                );
                assert!(
                    serde_json::from_str::<TickerEvent>(&text).is_err(),
                    "{text}"
                );
            }
        }
        for text in [
            r#"{"data":{"symbol":"BTCUSDT","lastPrice":"100"}} {}"#,
            r#"{"data":{"symbol":"BTCUSDT","lastPrice":"100","last\u0050rice":"101"}}"#,
            r#"{"data":{"symbol":"BTCUSDT","delta":"1","d\u0065lta":"2"}}"#,
        ] {
            assert!(serde_json::from_str::<LegacyTickerEvent>(text).is_err());
            assert!(serde_json::from_str::<TickerEvent>(text).is_err());
        }
    }
}
