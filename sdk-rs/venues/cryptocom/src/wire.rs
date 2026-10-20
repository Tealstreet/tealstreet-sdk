use safe_cex_engine::model::{Candle, Limits, Market, Min, MinMax, Precision, Ticker};
use safe_cex_engine::num;
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(default)]
    pub id: i64,
    #[serde(default)]
    pub method: &'a str,
    #[serde(default)]
    pub code: i64,
    #[serde(default)]
    pub message: &'a str,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
}

#[derive(Deserialize)]
pub struct AuthenticationReply {
    pub code: i64,
}

// The common public path decodes its nested payload once.
#[derive(Deserialize)]
pub struct TickerPush<'a> {
    #[serde(default)]
    pub code: i64,
    #[serde(borrow)]
    pub result: TickerMessage<'a>,
}

#[derive(Deserialize)]
pub enum TickerChannel {
    #[serde(rename = "ticker")]
    Ticker,
}

#[derive(Deserialize)]
pub struct TickerMessage<'a> {
    pub channel: TickerChannel,
    pub instrument_name: &'a str,
    #[serde(borrow)]
    pub data: Vec<TickerRow<'a>>,
}

#[derive(Deserialize)]
pub struct Message<'a> {
    pub channel: &'a str,
    #[serde(default)]
    pub subscription: &'a str,
    #[serde(default)]
    pub instrument_name: &'a str,
    #[serde(borrow)]
    pub data: &'a RawValue,
}

#[derive(Deserialize)]
pub struct InstrumentKey<'a> {
    #[serde(default)]
    pub symbol: Option<&'a str>,
    #[serde(default)]
    pub inst_type: Option<&'a str>,
}

#[derive(Deserialize)]
pub struct Instrument<'a> {
    pub symbol: &'a str,
    pub inst_type: &'a str,
    pub tradable: bool,
    pub base_ccy: &'a str,
    pub quote_ccy: &'a str,
    #[serde(default)]
    pub underlying_symbol: Option<&'a str>,
    #[serde(default)]
    pub price_tick_size: Option<&'a str>,
    #[serde(default)]
    pub qty_tick_size: Option<&'a str>,
    pub quote_decimals: i32,
    pub quantity_decimals: i32,
    pub max_leverage: &'a str,
    #[serde(default)]
    pub contract_size: Option<&'a str>,
}

impl Instrument<'_> {
    pub fn market(&self) -> Option<Market> {
        if self.inst_type != "PERPETUAL_SWAP" || !self.tradable {
            return None;
        }
        let qty = self.qty_tick_size.map_or(0.0, num::parse);
        let price = self.price_tick_size.map_or(0.0, num::parse);
        Some(Market {
            id: self.symbol.into(),
            symbol: self.symbol.into(),
            pretty_symbol: None,
            base: self.base_ccy.into(),
            quote: self.quote_ccy.into(),
            active: true,
            precision: Precision {
                amount: if qty > 0.0 {
                    qty
                } else {
                    10f64.powi(-self.quantity_decimals)
                },
                price: if price > 0.0 {
                    price
                } else {
                    10f64.powi(-self.quote_decimals)
                },
            },
            limits: Limits {
                amount: MinMax { min: qty, max: 0.0 },
                leverage: MinMax {
                    min: 1.0,
                    max: num::parse(self.max_leverage),
                },
                price: Some(Min { min: 0.0 }),
            },
            contract_size: self
                .contract_size
                .map(num::parse)
                .filter(|n| *n > 0.0)
                .or(Some(1.0)),
            is_expiring: Some(false),
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        })
    }
}

#[derive(Deserialize)]
pub struct TickerName<'a> {
    pub i: &'a str,
}

#[derive(Deserialize)]
pub struct TickerRow<'a> {
    pub i: &'a str,
    #[serde(default)]
    pub b: Option<&'a str>,
    #[serde(default)]
    pub k: Option<&'a str>,
    #[serde(default)]
    pub a: Option<&'a str>,
    #[serde(default)]
    pub c: Option<&'a str>,
    #[serde(default)]
    pub v: Option<&'a str>,
    #[serde(default)]
    pub vv: Option<&'a str>,
    #[serde(default)]
    pub oi: Option<&'a str>,
}

impl TickerRow<'_> {
    pub fn apply(&self, t: &mut Ticker) {
        t.bid = self.b.map_or(0.0, num::parse);
        t.ask = self.k.map_or(0.0, num::parse);
        t.last = self.a.map_or(0.0, num::parse);
        t.percentage = self.c.map_or(0.0, num::parse) * 100.0;
        t.volume = self.v.map_or(0.0, num::parse);
        t.quote_volume = self.vv.map_or(0.0, num::parse);
        t.open_interest = self.oi.map_or(0.0, num::parse);
    }
}

#[derive(Deserialize)]
pub struct BookRow<'a> {
    #[serde(borrow)]
    pub bids: Vec<(&'a str, &'a str, &'a RawValue)>,
    #[serde(borrow)]
    pub asks: Vec<(&'a str, &'a str, &'a RawValue)>,
}

#[derive(Deserialize)]
pub struct Valuation<'a> {
    pub v: &'a str,
}

#[derive(Deserialize)]
pub struct CandleRow<'a> {
    pub t: f64,
    pub o: &'a str,
    pub h: &'a str,
    pub l: &'a str,
    pub c: &'a str,
    pub v: &'a str,
}
impl CandleRow<'_> {
    pub fn candle(&self) -> Candle {
        Candle {
            timestamp: self.t,
            open: num::parse(self.o),
            high: num::parse(self.h),
            low: num::parse(self.l),
            close: num::parse(self.c),
            volume: num::parse(self.v),
        }
    }
}

#[derive(Deserialize)]
pub struct RestEnvelope<'a> {
    pub code: i64,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
    #[serde(default)]
    pub message: &'a str,
}
