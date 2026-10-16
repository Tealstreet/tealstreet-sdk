//! Native market/holding/order/options declarations from polymarket.types.ts.
//! Text preserves UTF-16; Number preserves f64; nullable and optional are distinct.
//! JSON text, general JS object behavior, signing and exchange composition are separate.
use crate::ws_records::{Decode, DecodeError, NativeValue, Optional, OrderType, Side, Text};
#[derive(Debug, Clone, PartialEq)]
pub enum Nullable<T> {
    Null,
    Value(T),
}
impl<T: Decode> Decode for Nullable<T> {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Null => Ok(Self::Null),
            _ => T::decode(value).map(Self::Value),
        }
    }
}
#[derive(Debug, Clone, PartialEq)]
pub enum NumberOrText {
    Number(f64),
    Text(Text),
}
impl Decode for NumberOrText {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        match value {
            NativeValue::Number(v) => Ok(Self::Number(*v)),
            NativeValue::Text(v) => Ok(Self::Text(Text(v.clone()))),
            _ => Err(error("", "number or string")),
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Outcome {
    Yes,
    No,
}
impl Decode for Outcome {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let text = Text::decode(value)?.0;
        if text == "Yes".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::Yes);
        }
        if text == "No".encode_utf16().collect::<Vec<_>>() {
            return Ok(Self::No);
        }
        Err(error("", "Yes or No"))
    }
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
fn required<T: Decode>(fields: &[(String, NativeValue)], key: &str) -> Result<T, DecodeError> {
    let value = fields
        .iter()
        .find(|(name, _)| name == key)
        .map_or(&NativeValue::Undefined, |(_, value)| value);
    T::decode(value).map_err(|mut e| {
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

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketToken {
    pub token_id: Text,
    pub outcome: Outcome,
    pub winner: Optional<bool>,
}
impl Decode for PolymarketToken {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            token_id: required(fields, "token_id")?,
            outcome: required(fields, "outcome")?,
            winner: optional(fields, "winner")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketGammaMarket {
    pub condition_id: Text,
    pub question_id: Text,
    pub tokens: Vec<PolymarketToken>,
    pub question: Text,
    pub description: Text,
    pub market_slug: Text,
    pub end_date_iso: Text,
    pub active: bool,
    pub closed: bool,
    pub archived: bool,
    pub minimum_order_size: Text,
    pub minimum_tick_size: Text,
    pub tags: Optional<Vec<Text>>,
    pub volume: Optional<Text>,
    pub volume_24h: Optional<Text>,
    pub liquidity: Optional<Text>,
    pub unique_traders: Optional<f64>,
}
impl Decode for PolymarketGammaMarket {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            condition_id: required(fields, "condition_id")?,
            question_id: required(fields, "question_id")?,
            tokens: required(fields, "tokens")?,
            question: required(fields, "question")?,
            description: required(fields, "description")?,
            market_slug: required(fields, "market_slug")?,
            end_date_iso: required(fields, "end_date_iso")?,
            active: required(fields, "active")?,
            closed: required(fields, "closed")?,
            archived: required(fields, "archived")?,
            minimum_order_size: required(fields, "minimum_order_size")?,
            minimum_tick_size: required(fields, "minimum_tick_size")?,
            tags: optional(fields, "tags")?,
            volume: optional(fields, "volume")?,
            volume_24h: optional(fields, "volume_24h")?,
            liquidity: optional(fields, "liquidity")?,
            unique_traders: optional(fields, "unique_traders")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketPosition {
    pub token_id: Text,
    pub condition_id: Text,
    pub outcome: Text,
    pub size: Text,
    pub avg_price: Text,
    pub market_slug: Optional<Text>,
    pub question: Optional<Text>,
    pub current_price: Optional<Text>,
    pub unrealized_pnl: Optional<Text>,
}
impl Decode for PolymarketPosition {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            token_id: required(fields, "token_id")?,
            condition_id: required(fields, "condition_id")?,
            outcome: required(fields, "outcome")?,
            size: required(fields, "size")?,
            avg_price: required(fields, "avg_price")?,
            market_slug: optional(fields, "market_slug")?,
            question: optional(fields, "question")?,
            current_price: optional(fields, "current_price")?,
            unrealized_pnl: optional(fields, "unrealized_pnl")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketSignedOrder {
    pub salt: Text,
    pub maker: Text,
    pub signer: Text,
    pub token_id: Text,
    pub maker_amount: Text,
    pub taker_amount: Text,
    pub expiration: Text,
    pub side: Side,
    pub signature_type: f64,
    pub timestamp: Text,
    pub metadata: Text,
    pub builder: Text,
    pub signature: Text,
}
impl Decode for PolymarketSignedOrder {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            salt: required(fields, "salt")?,
            maker: required(fields, "maker")?,
            signer: required(fields, "signer")?,
            token_id: required(fields, "tokenId")?,
            maker_amount: required(fields, "makerAmount")?,
            taker_amount: required(fields, "takerAmount")?,
            expiration: required(fields, "expiration")?,
            side: required(fields, "side")?,
            signature_type: required(fields, "signatureType")?,
            timestamp: required(fields, "timestamp")?,
            metadata: required(fields, "metadata")?,
            builder: required(fields, "builder")?,
            signature: required(fields, "signature")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketCreateOrderRequest {
    pub order: PolymarketSignedOrder,
    pub owner: Text,
    pub order_type: OrderType,
}
impl Decode for PolymarketCreateOrderRequest {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            order: required(fields, "order")?,
            owner: required(fields, "owner")?,
            order_type: required(fields, "orderType")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketCancelRequest {
    pub order_ids: Optional<Vec<Text>>,
    pub market: Optional<Text>,
    pub asset_id: Optional<Text>,
}
impl Decode for PolymarketCancelRequest {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            order_ids: optional(fields, "orderIds")?,
            market: optional(fields, "market")?,
            asset_id: optional(fields, "asset_id")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketExchangeOptions {
    pub api_key: Optional<Text>,
    pub api_secret: Optional<Text>,
    pub api_passphrase: Optional<Text>,
    pub signer_type: Optional<f64>,
    pub funder_address: Optional<Text>,
}
impl Decode for PolymarketExchangeOptions {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            api_key: optional(fields, "apiKey")?,
            api_secret: optional(fields, "apiSecret")?,
            api_passphrase: optional(fields, "apiPassphrase")?,
            signer_type: optional(fields, "signerType")?,
            funder_address: optional(fields, "funderAddress")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct GammaMarketInfo {
    pub id: Text,
    pub question: Text,
    pub condition_id: Text,
    pub slug: Text,
    pub resolution_source: Text,
    pub end_date: Text,
    pub liquidity: Text,
    pub start_date: Text,
    pub fee: Text,
    pub description: Text,
    pub outcomes: Nullable<Text>,
    pub outcome_prices: Nullable<Text>,
    pub volume: Text,
    pub active: bool,
    pub closed: bool,
    pub market_type: Text,
    pub clob_token_ids: Nullable<Text>,
    pub order_price_min_tick_size: Nullable<NumberOrText>,
    pub order_min_size: Nullable<NumberOrText>,
    pub neg_risk: Optional<Nullable<bool>>,
    pub enable_order_book: Optional<Nullable<bool>>,
    pub accepting_orders: Optional<Nullable<bool>>,
}
impl Decode for GammaMarketInfo {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            id: required(fields, "id")?,
            question: required(fields, "question")?,
            condition_id: required(fields, "conditionId")?,
            slug: required(fields, "slug")?,
            resolution_source: required(fields, "resolutionSource")?,
            end_date: required(fields, "endDate")?,
            liquidity: required(fields, "liquidity")?,
            start_date: required(fields, "startDate")?,
            fee: required(fields, "fee")?,
            description: required(fields, "description")?,
            outcomes: required(fields, "outcomes")?,
            outcome_prices: required(fields, "outcomePrices")?,
            volume: required(fields, "volume")?,
            active: required(fields, "active")?,
            closed: required(fields, "closed")?,
            market_type: required(fields, "marketType")?,
            clob_token_ids: required(fields, "clobTokenIds")?,
            order_price_min_tick_size: required(fields, "orderPriceMinTickSize")?,
            order_min_size: required(fields, "orderMinSize")?,
            neg_risk: optional(fields, "negRisk")?,
            enable_order_book: optional(fields, "enableOrderBook")?,
            accepting_orders: optional(fields, "acceptingOrders")?,
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct PolymarketDataPosition {
    pub proxy_wallet: Text,
    pub asset: Text,
    pub condition_id: Text,
    pub size: f64,
    pub avg_price: f64,
    pub current_value: f64,
    pub cash_pnl: f64,
    pub cur_price: f64,
    pub redeemable: bool,
    pub mergeable: bool,
    pub title: Text,
    pub slug: Text,
    pub outcome: Text,
    pub negative_risk: bool,
}
impl Decode for PolymarketDataPosition {
    fn decode(value: &NativeValue) -> Result<Self, DecodeError> {
        let fields = object(value)?;
        Ok(Self {
            proxy_wallet: required(fields, "proxyWallet")?,
            asset: required(fields, "asset")?,
            condition_id: required(fields, "conditionId")?,
            size: required(fields, "size")?,
            avg_price: required(fields, "avgPrice")?,
            current_value: required(fields, "currentValue")?,
            cash_pnl: required(fields, "cashPnl")?,
            cur_price: required(fields, "curPrice")?,
            redeemable: required(fields, "redeemable")?,
            mergeable: required(fields, "mergeable")?,
            title: required(fields, "title")?,
            slug: required(fields, "slug")?,
            outcome: required(fields, "outcome")?,
            negative_risk: required(fields, "negativeRisk")?,
        })
    }
}
