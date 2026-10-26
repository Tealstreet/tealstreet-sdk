//! Borrowed public payloads; lists are decoded one row at a time.
use serde::Deserialize;
use std::borrow::Cow;

pub use safe_cex_engine::de::{number, optional_number};
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow)]
    pub data: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    pub ticker: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    pub market_stats: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    pub order_book: Option<&'a serde_json::value::RawValue>,
    #[serde(default)]
    pub error: Option<ApiError<'a>>,
}
#[derive(Deserialize)]
pub struct ApiError<'a> {
    #[serde(default)]
    pub code: i64,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct Status<'a> {
    pub code: i64,
    #[serde(default, borrow)]
    pub message: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct Instrument<'a> {
    #[serde(deserialize_with = "number")]
    pub market_id: f64,
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub status: Cow<'a, str>,
    #[serde(default, alias = "marketType", borrow)]
    pub market_type: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub supported_size_decimals: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub supported_price_decimals: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub min_base_amount: f64,
    #[serde(default, deserialize_with = "number")]
    pub min_quote_amount: f64,
    #[serde(default, deserialize_with = "optional_number")]
    pub order_quote_limit: Option<f64>,
    #[serde(
        default,
        alias = "minInitialMarginFraction",
        deserialize_with = "optional_number"
    )]
    pub min_initial_margin_fraction: Option<f64>,
    #[serde(
        default,
        alias = "defaultInitialMarginFraction",
        deserialize_with = "optional_number"
    )]
    pub default_initial_margin_fraction: Option<f64>,
    #[serde(default, deserialize_with = "number")]
    pub maintenance_margin_fraction: f64,
    #[serde(default)]
    pub market_config: Option<MarketConfig>,
    #[serde(default, deserialize_with = "optional_number")]
    pub last_trade_price: Option<f64>,
}
#[derive(Deserialize)]
pub struct MarketConfig {
    #[serde(default)]
    pub market_margin_mode: u8,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum Level {
    Object {
        #[serde(deserialize_with = "number")]
        price: f64,
        #[serde(
            default,
            alias = "remaining_base_amount",
            alias = "amount",
            deserialize_with = "number"
        )]
        size: f64,
    },
    Tuple(
        #[serde(deserialize_with = "number")] f64,
        #[serde(deserialize_with = "number")] f64,
    ),
}
impl Level {
    pub fn pair(&self) -> (f64, f64) {
        match self {
            Self::Object { price, size } => (*price, *size),
            Self::Tuple(p, s) => (*p, *s),
        }
    }
}
#[derive(Default, Deserialize)]
pub struct Depth {
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
    #[serde(default)]
    pub nonce: Option<u64>,
    #[serde(default)]
    pub begin_nonce: Option<u64>,
}
#[derive(Deserialize)]
#[serde(untagged)]
pub enum BestPrice {
    Number(#[serde(deserialize_with = "number")] f64),
    Object {
        #[serde(
            alias = "p",
            alias = "bestPrice",
            alias = "best_price",
            deserialize_with = "number"
        )]
        price: f64,
    },
    Tuple(Vec<Numeric>),
    Text(String),
}
impl BestPrice {
    pub fn price(&self) -> Option<f64> {
        match self {
            Self::Number(p) | Self::Object { price: p } => Some(*p),
            Self::Tuple(v) => v.first().map(|n| n.0),
            Self::Text(s) => s.parse::<f64>().ok().filter(|n| n.is_finite()),
        }
    }
}
#[derive(Deserialize)]
pub struct Numeric(#[serde(deserialize_with = "number")] pub f64);
#[derive(Default, Deserialize)]
pub struct Stats<'a> {
    #[serde(
        default,
        alias = "marketSymbol",
        alias = "market_symbol",
        alias = "s",
        borrow
    )]
    pub symbol: Cow<'a, str>,
    #[serde(default, alias = "marketId", deserialize_with = "optional_number")]
    pub market_id: Option<f64>,
    #[serde(
        default,
        alias = "markPrice",
        alias = "mark",
        deserialize_with = "optional_number"
    )]
    pub mark_price: Option<f64>,
    #[serde(
        default,
        alias = "indexPrice",
        alias = "index",
        deserialize_with = "optional_number"
    )]
    pub index_price: Option<f64>,
    #[serde(
        default,
        alias = "lastPrice",
        alias = "last_price",
        alias = "lastTradePrice",
        alias = "last",
        alias = "price",
        deserialize_with = "optional_number"
    )]
    pub last_trade_price: Option<f64>,
    #[serde(
        default,
        alias = "bid_price",
        alias = "bidPrice",
        alias = "bestBidPrice",
        alias = "best_bid",
        alias = "bestBid"
    )]
    pub best_bid_price: Option<BestPrice>,
    #[serde(
        default,
        alias = "ask_price",
        alias = "askPrice",
        alias = "bestAskPrice",
        alias = "best_ask",
        alias = "bestAsk"
    )]
    pub best_ask_price: Option<BestPrice>,
    #[serde(default, alias = "bid")]
    pub b: Option<BestPrice>,
    #[serde(default, alias = "ask")]
    pub a: Option<BestPrice>,
    #[serde(
        default,
        alias = "openInterest",
        alias = "oi",
        deserialize_with = "optional_number"
    )]
    pub open_interest: Option<f64>,
    #[serde(
        default,
        alias = "openInterestUsd",
        deserialize_with = "optional_number"
    )]
    pub open_interest_usd: Option<f64>,
    #[serde(
        default,
        alias = "fundingRate",
        alias = "currentFundingRate",
        deserialize_with = "optional_number"
    )]
    pub current_funding_rate: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub funding_rate: Option<f64>,
    #[serde(
        default,
        alias = "volume",
        alias = "volume24h",
        alias = "volume_24h",
        deserialize_with = "optional_number"
    )]
    pub daily_base_token_volume: Option<f64>,
    #[serde(
        default,
        alias = "quote_volume",
        alias = "quoteVolume",
        alias = "quoteVolume24h",
        alias = "quote_volume_24h",
        deserialize_with = "optional_number"
    )]
    pub daily_quote_token_volume: Option<f64>,
    #[serde(
        default,
        alias = "priceChange24h",
        alias = "price_change_24h",
        deserialize_with = "optional_number"
    )]
    pub daily_price_change: Option<f64>,
}
#[derive(Deserialize)]
pub struct CandleRow {
    #[serde(alias = "timestamp", deserialize_with = "number")]
    pub t: f64,
    #[serde(alias = "open", deserialize_with = "number")]
    pub o: f64,
    #[serde(alias = "high", deserialize_with = "number")]
    pub h: f64,
    #[serde(alias = "low", deserialize_with = "number")]
    pub l: f64,
    #[serde(alias = "close", deserialize_with = "number")]
    pub c: f64,
    #[serde(alias = "volume", deserialize_with = "number")]
    pub v: f64,
}
impl CandleRow {
    pub fn candle(&self) -> safe_cex_engine::model::Candle {
        safe_cex_engine::model::Candle {
            timestamp: if self.t > 10_000_000_000.0 {
                self.t / 1000.0
            } else {
                self.t
            },
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}
#[derive(Deserialize)]
pub struct CandleMessage {
    #[serde(default)]
    pub candles: Vec<CandleRow>,
    #[serde(default)]
    pub candle: Option<CandleRow>,
}
#[derive(Deserialize)]
pub struct TradeRow {
    #[serde(alias = "t", deserialize_with = "number")]
    pub timestamp: f64,
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub size: f64,
}
#[derive(Deserialize)]
pub struct TradeMessage {
    #[serde(default)]
    pub trades: Vec<TradeRow>,
    #[serde(default)]
    pub trade: Option<TradeRow>,
}

// A malformed market must not discard the other rows in an all-market frame.
pub fn each_stats_map<'de>(
    text: &'de str,
    mut each: impl FnMut(&str, serde_json::Result<Stats<'de>>),
) -> serde_json::Result<()> {
    struct Rows<'f, F>(&'f mut F);
    impl<'de, F: FnMut(&str, serde_json::Result<Stats<'de>>)> serde::de::Visitor<'de> for Rows<'_, F> {
        type Value = ();
        fn expecting(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
            f.write_str("market statistics keyed by market ID")
        }
        fn visit_map<M: serde::de::MapAccess<'de>>(self, mut map: M) -> Result<(), M::Error> {
            while let Some(key) = map.next_key::<Cow<'de, str>>()? {
                let raw = map.next_value::<&'de serde_json::value::RawValue>()?;
                (self.0)(&key, serde_json::from_str(raw.get()));
            }
            Ok(())
        }
    }
    let mut deserializer = serde_json::Deserializer::from_str(text);
    serde::Deserializer::deserialize_map(&mut deserializer, Rows(&mut each))?;
    deserializer.end()
}
