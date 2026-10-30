//! PerpetualV2 public wire records use real prices and quantities.
use safe_cex_engine::model::{Candle, Interval, Limits, Market, MinMax, Precision, Ticker};
use serde::Deserialize;
use std::borrow::Cow;
pub type Str<'a> = Cow<'a, str>;
pub use safe_cex_engine::de::{Number as N, number, optional_number};
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Product<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(rename = "type", borrow)]
    pub kind: Str<'a>,
    #[serde(borrow)]
    pub settle_currency: Str<'a>,
    #[serde(borrow)]
    pub base_currency: Str<'a>,
    #[serde(borrow)]
    pub quote_currency: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub qty_step_size: f64,
    #[serde(deserialize_with = "number")]
    pub tick_size: f64,
    #[serde(deserialize_with = "number")]
    pub max_order_qty_rq: f64,
    #[serde(deserialize_with = "number")]
    pub max_leverage: f64,
}
impl Product<'_> {
    pub fn supported(&self) -> bool {
        self.kind == "PerpetualV2" && matches!(self.settle_currency.as_ref(), "USDT" | "USDC")
    }
    pub fn valid(&self) -> bool {
        self.qty_step_size > 0.
            && self.tick_size > 0.
            && self.max_order_qty_rq >= self.qty_step_size
            && self.max_leverage >= 1.
            && !self.symbol.is_empty()
            && !self.base_currency.is_empty()
            && !self.quote_currency.is_empty()
    }

    pub fn market(self) -> Option<Market> {
        if !self.supported() || !self.valid() {
            return None;
        }
        Some(Market {
            id: format!(
                "{}/{}:{}",
                self.base_currency, self.quote_currency, self.settle_currency
            ),
            symbol: self.symbol.to_ascii_uppercase(),
            pretty_symbol: None,
            base: self.base_currency.into_owned(),
            quote: self.quote_currency.into_owned(),
            active: true,
            precision: Precision {
                amount: self.qty_step_size,
                price: self.tick_size,
            },
            limits: Limits {
                amount: MinMax {
                    min: 0.,
                    max: self.max_order_qty_rq,
                },
                leverage: MinMax {
                    min: 1.,
                    max: self.max_leverage,
                },
                price: None,
            },
            contract_size: None,
            is_expiring: None,
            is_inverse: None,
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        })
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(deserialize_with = "number")]
    pub bid_rp: f64,
    #[serde(deserialize_with = "number")]
    pub ask_rp: f64,
    #[serde(deserialize_with = "number")]
    pub last_rp: f64,
    #[serde(deserialize_with = "number")]
    pub mark_rp: f64,
    #[serde(deserialize_with = "number")]
    pub index_rp: f64,
    #[serde(deserialize_with = "number")]
    pub open_rp: f64,
    #[serde(deserialize_with = "number")]
    pub open_interest_rv: f64,
    #[serde(deserialize_with = "number")]
    pub funding_rate_rr: f64,
    #[serde(deserialize_with = "number")]
    pub volume_rq: f64,
}
impl Daily<'_> {
    pub fn ticker(&self, id: String) -> Ticker {
        Ticker {
            id,
            symbol: self.symbol.to_ascii_uppercase(),
            bid: self.bid_rp,
            ask: self.ask_rp,
            last: self.last_rp,
            mark: self.mark_rp,
            index: self.index_rp,
            percentage: percentage(self.last_rp, self.open_rp),
            open_interest: self.open_interest_rv,
            funding_rate: self.funding_rate_rr,
            volume: self.volume_rq,
            quote_volume: self.volume_rq * self.last_rp,
            ..Ticker::default()
        }
    }
}
pub fn percentage(last: f64, open: f64) -> f64 {
    if open == 0. {
        0.
    } else {
        (last - open) / open * 100.
    }
}
#[derive(Deserialize)]
pub struct Packed<'a>(
    #[serde(borrow)] pub Str<'a>,
    pub N,
    serde::de::IgnoredAny,
    serde::de::IgnoredAny,
    pub N,
    pub N,
    serde::de::IgnoredAny,
    pub N,
    pub N,
    pub N,
    pub N,
    serde::de::IgnoredAny,
    #[serde(default)] pub Option<N>,
    #[serde(default)] pub Option<N>,
);
#[derive(Deserialize)]
pub struct Level(pub N, pub N);
#[derive(Default, Deserialize)]
pub struct Depth {
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Snapshot {
    pub result: SnapshotData,
}
#[derive(Deserialize)]
pub struct SnapshotData {
    pub sequence: u64,
    pub orderbook_p: SnapshotDepth,
}
#[derive(Deserialize)]
pub struct SnapshotDepth {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
impl From<SnapshotDepth> for Depth {
    fn from(d: SnapshotDepth) -> Self {
        Self {
            bids: d.bids,
            asks: d.asks,
        }
    }
}
#[derive(Deserialize)]
pub struct DepthEvent<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub sequence: u64,
    #[serde(rename = "type", default, borrow)]
    pub kind: Str<'a>,
    pub orderbook_p: Depth,
}
#[derive(Deserialize)]
pub struct CandleRow(
    pub N,
    pub N,
    serde::de::IgnoredAny,
    pub N,
    pub N,
    pub N,
    pub N,
    pub N,
    serde::de::IgnoredAny,
    // REST rows append the symbol; socket `kline_p` rows stop at turnover.
    #[serde(default)] serde::de::IgnoredAny,
);
impl CandleRow {
    pub fn candle(&self) -> Candle {
        Candle {
            timestamp: self.0.0,
            open: self.3.0,
            high: self.4.0,
            low: self.5.0,
            close: self.6.0,
            volume: self.7.0,
        }
    }
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default, borrow)]
    pub orderbook_p: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    pub kline_p: Option<&'a serde_json::value::RawValue>,
    #[serde(default, borrow)]
    pub method: Str<'a>,
    #[serde(default, borrow)]
    pub symbol: Str<'a>,
    #[serde(default, borrow)]
    pub error: Option<&'a serde_json::value::RawValue>,
    #[serde(default)]
    pub code: Option<i64>,
    #[serde(default, borrow)]
    pub msg: Str<'a>,
}
/// Phemex kline resolution in seconds.
pub fn resolution(i: Interval) -> Option<u32> {
    use Interval::*;
    matches!(
        i,
        M1 | M3 | M5 | M15 | M30 | H1 | H2 | H3 | H4 | H6 | H12 | D1 | W1 | Mo1
    )
    .then(|| (i.ms() / 1000.0) as u32)
}

pub fn snapshot_sides(text: &str) -> bool {
    #[derive(Deserialize)]
    struct Sides<'a> {
        #[serde(rename = "bids", borrow)]
        _bids: &'a serde_json::value::RawValue,
        #[serde(rename = "asks", borrow)]
        _asks: &'a serde_json::value::RawValue,
    }
    serde_json::from_str::<Sides<'_>>(text).is_ok()
}
