//! Native mirrors of the eight websocket declarations and their nested fields.
//! Text is UTF-16, Number is f64. Projection consumes a native value tree;
//! JSON text parsing, transport, handlers and general JS objects are separate.
#[derive(Debug, Clone, PartialEq)]
pub enum NativeValue {
    Undefined,
    Null,
    Text(Vec<u16>),
    Number(f64),
    Bool(bool),
    Array(Vec<NativeValue>),
    Object(Vec<(String, NativeValue)>),
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DecodeError {
    pub field: String,
    pub expected: &'static str,
}
/// Typed, densely populated source records at a native runtime boundary.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Text(pub Vec<u16>);
/// Optional property presence: omission and present undefined remain distinct.
#[derive(Debug, Clone, PartialEq)]
pub enum Optional<T> {
    Absent,
    Undefined,
    Value(T),
}
pub trait Decode: Sized {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError>;
}
fn error(field: &str, expected: &'static str) -> DecodeError {
    DecodeError {
        field: field.into(),
        expected,
    }
}
fn object(value: &NativeValue) -> Result<&[(String, NativeValue)], DecodeError> {
    match value {
        NativeValue::Object(fields) => Ok(fields),
        _ => Err(error("", "record")),
    }
}
fn field<'a>(fields: &'a [(String, NativeValue)], key: &str) -> &'a NativeValue {
    fields
        .iter()
        .find(|(name, _)| name == key)
        .map_or(&NativeValue::Undefined, |(_, value)| value)
}
fn required<T: Decode>(fields: &[(String, NativeValue)], key: &str) -> Result<T, DecodeError> {
    T::decode(field(fields, key)).map_err(|mut e| {
        e.field = if e.field.is_empty() {
            key.into()
        } else {
            format!("{key}.{}", e.field)
        };
        e
    })
}
fn optional<T: Decode>(
    fields: &[(String, NativeValue)],
    key: &str,
) -> Result<Optional<T>, DecodeError> {
    match fields.iter().find(|(name, _)| name == key) {
        None => Ok(Optional::Absent),
        Some((_, NativeValue::Undefined)) => Ok(Optional::Undefined),
        Some(_) => required(fields, key).map(Optional::Value),
    }
}

impl Decode for Text {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Text(v) => Ok(Text(v.clone())),
            _ => Err(error("", "string")),
        }
    }
}
impl Decode for f64 {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Number(v) => Ok(*v),
            _ => Err(error("", "number")),
        }
    }
}
impl Decode for bool {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Bool(v) => Ok(*v),
            _ => Err(error("", "boolean")),
        }
    }
}
impl<T: Decode> Decode for Vec<T> {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Array(values) => values
                .iter()
                .enumerate()
                .map(|(i, v)| {
                    T::decode(v).map_err(|mut e| {
                        e.field = if e.field.is_empty() {
                            i.to_string()
                        } else {
                            format!("{i}.{}", e.field)
                        };
                        e
                    })
                })
                .collect(),
            _ => Err(error("", "array")),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Side {
    Buy,
    Sell,
}
impl Decode for Side {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "BUY".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Buy);
        }
        if text == "SELL".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Sell);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Status {
    Live,
    Matched,
    Cancelled,
    Delayed,
}
impl Decode for Status {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "live".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Live);
        }
        if text == "matched".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Matched);
        }
        if text == "cancelled".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Cancelled);
        }
        if text == "delayed".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Delayed);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderType {
    Gtc,
    Fok,
    Gtd,
    Fak,
}
impl Decode for OrderType {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "GTC".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Gtc);
        }
        if text == "FOK".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Fok);
        }
        if text == "GTD".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Gtd);
        }
        if text == "FAK".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Fak);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Channel {
    User,
    Market,
}
impl Decode for Channel {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "user".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::User);
        }
        if text == "market".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Market);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Operation {
    Subscribe,
    Unsubscribe,
}
impl Decode for Operation {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "subscribe".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Subscribe);
        }
        if text == "unsubscribe".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Unsubscribe);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PriceEvent {
    PriceChange,
}
impl Decode for PriceEvent {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "price_change".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::PriceChange);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BookEvent {
    Book,
}
impl Decode for BookEvent {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "book".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Book);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TradeEvent {
    Trade,
}
impl Decode for TradeEvent {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "trade".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Trade);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OrderEvent {
    Order,
}
impl Decode for OrderEvent {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "order".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Order);
        }
        Err(error("", "declared string literal"))
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketTrade {
    pub id: Text,
    pub taker_order_id: Text,
    pub maker_order_id: Text,
    pub market: Text,
    pub asset_id: Text,
    pub side: Side,
    pub size: Text,
    pub price: Text,
    pub status: Text,
    pub match_time: f64,
    pub transaction_hash: Optional<Text>,
    pub fee_rate_bps: Optional<Text>,
    pub bucket_index: Optional<f64>,
    pub maker_address: Optional<Text>,
}
impl Decode for PolymarketTrade {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            id: required(fields, "id")?,
            taker_order_id: required(fields, "taker_order_id")?,
            maker_order_id: required(fields, "maker_order_id")?,
            market: required(fields, "market")?,
            asset_id: required(fields, "asset_id")?,
            side: required(fields, "side")?,
            size: required(fields, "size")?,
            price: required(fields, "price")?,
            status: required(fields, "status")?,
            match_time: required(fields, "match_time")?,
            transaction_hash: optional(fields, "transaction_hash")?,
            fee_rate_bps: optional(fields, "fee_rate_bps")?,
            bucket_index: optional(fields, "bucket_index")?,
            maker_address: optional(fields, "maker_address")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketAPIOrder {
    pub id: Text,
    pub owner: Text,
    pub market: Text,
    pub asset_id: Text,
    pub side: Side,
    pub original_size: Text,
    pub size_matched: Text,
    pub price: Text,
    pub status: Status,
    pub created_at: f64,
    pub expiration: f64,
    pub order_type: OrderType,
    pub associate_trades: Optional<Vec<PolymarketTrade>>,
}
impl Decode for PolymarketAPIOrder {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            id: required(fields, "id")?,
            owner: required(fields, "owner")?,
            market: required(fields, "market")?,
            asset_id: required(fields, "asset_id")?,
            side: required(fields, "side")?,
            original_size: required(fields, "original_size")?,
            size_matched: required(fields, "size_matched")?,
            price: required(fields, "price")?,
            status: required(fields, "status")?,
            created_at: required(fields, "created_at")?,
            expiration: required(fields, "expiration")?,
            order_type: required(fields, "order_type")?,
            associate_trades: optional(fields, "associate_trades")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct AuthFields {
    pub api_key: Text,
    pub secret: Text,
    pub passphrase: Text,
}
impl Decode for AuthFields {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            api_key: required(fields, "apiKey")?,
            secret: required(fields, "secret")?,
            passphrase: required(fields, "passphrase")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PriceChange {
    pub asset_id: Text,
    pub price: Text,
    pub size: Text,
    pub side: Side,
    pub best_bid: Text,
    pub best_ask: Text,
}
impl Decode for PriceChange {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            asset_id: required(fields, "asset_id")?,
            price: required(fields, "price")?,
            size: required(fields, "size")?,
            side: required(fields, "side")?,
            best_bid: required(fields, "best_bid")?,
            best_ask: required(fields, "best_ask")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct BookLevel {
    pub price: Text,
    pub size: Text,
}
impl Decode for BookLevel {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            price: required(fields, "price")?,
            size: required(fields, "size")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSSubscription {
    pub auth: Optional<AuthFields>,
    pub markets: Optional<Vec<Text>>,
    pub assets_ids: Optional<Vec<Text>>,
    pub r#type: Channel,
    pub custom_feature_enabled: Optional<bool>,
}
impl Decode for PolymarketWSSubscription {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            auth: optional(fields, "auth")?,
            markets: optional(fields, "markets")?,
            assets_ids: optional(fields, "assets_ids")?,
            r#type: required(fields, "type")?,
            custom_feature_enabled: optional(fields, "custom_feature_enabled")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSOperation {
    pub assets_ids: Optional<Vec<Text>>,
    pub markets: Optional<Vec<Text>>,
    pub operation: Operation,
    pub custom_feature_enabled: Optional<bool>,
}
impl Decode for PolymarketWSOperation {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            assets_ids: optional(fields, "assets_ids")?,
            markets: optional(fields, "markets")?,
            operation: required(fields, "operation")?,
            custom_feature_enabled: optional(fields, "custom_feature_enabled")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSPriceChange {
    pub event_type: PriceEvent,
    pub price_changes: Vec<PriceChange>,
    pub timestamp: f64,
}
impl Decode for PolymarketWSPriceChange {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            event_type: required(fields, "event_type")?,
            price_changes: required(fields, "price_changes")?,
            timestamp: required(fields, "timestamp")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSOrderBookUpdate {
    pub event_type: BookEvent,
    pub asset_id: Text,
    pub market: Text,
    pub bids: Optional<Vec<BookLevel>>,
    pub asks: Optional<Vec<BookLevel>>,
    pub timestamp: f64,
    pub hash: Optional<Text>,
}
impl Decode for PolymarketWSOrderBookUpdate {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            event_type: required(fields, "event_type")?,
            asset_id: required(fields, "asset_id")?,
            market: required(fields, "market")?,
            bids: optional(fields, "bids")?,
            asks: optional(fields, "asks")?,
            timestamp: required(fields, "timestamp")?,
            hash: optional(fields, "hash")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSTrade {
    pub trade: PolymarketTrade,
    pub event_type: TradeEvent,
}
impl Decode for PolymarketWSTrade {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            trade: PolymarketTrade::decode(value)?,
            event_type: required(fields, "event_type")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketWSOrderUpdate {
    pub order: PolymarketAPIOrder,
    pub event_type: OrderEvent,
}
impl Decode for PolymarketWSOrderUpdate {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            order: PolymarketAPIOrder::decode(value)?,
            event_type: required(fields, "event_type")?,
        })
    }
}
