//! TxFlow's public envelopes and streamed market records.
use safe_cex_engine::de::{number_or_zero, optional_number_or_blank};
use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision};
use serde::Deserialize;
use std::borrow::Cow;

/// Live overlay fields: null or missing leaves the field untouched, blank reads
/// as 0, malformed rejects the row.
fn optional_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<f64>, D::Error> {
    #[derive(Deserialize)]
    struct Zero(#[serde(deserialize_with = "number_or_zero")] f64);
    Ok(Option::<Zero>::deserialize(d)?.map(|n| n.0))
}

#[derive(Deserialize)]
pub struct Header {
    pub success: bool,
    pub code: i64,
}
impl Header {
    pub fn valid(body: &str) -> bool {
        serde_json::from_str::<Self>(body).is_ok_and(|h| h.success && h.code == 0)
    }
}
#[derive(Deserialize)]
pub struct Reply<T> {
    pub success: bool,
    pub code: i64,
    pub data: T,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    #[serde(borrow, alias = "type")]
    pub channel: Cow<'a, str>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}

#[derive(Deserialize)]
#[serde(tag = "channel", content = "data")]
pub enum PublicFrame<'a> {
    #[serde(rename = "allAssetCtxs", borrow)]
    Contexts(Contexts<'a>),
    #[serde(rename = "activeAssetCtx", borrow)]
    Context(ContextFrame<'a>),
    #[serde(rename = "ticker")]
    Ticker(TickerRow),
    #[serde(rename = "l2Book", borrow)]
    Depth(Depth<'a>),
    #[serde(rename = "candle", borrow)]
    Candle(&'a serde_json::value::RawValue),
    #[serde(rename = "error", borrow)]
    Error(ErrorCode<'a>),
    #[serde(rename = "subscriptionResponse")]
    Acknowledgement(serde::de::IgnoredAny),
    #[serde(other)]
    Other,
}
#[derive(Deserialize)]
pub struct ErrorCode<'a> {
    #[serde(borrow, default)]
    pub code: Option<Cow<'a, str>>,
}
impl<'a> PublicFrame<'a> {
    pub fn decode(text: &'a str) -> Result<Self, serde_json::Error> {
        // Channel-first market frames decode their payload without scanning it twice.
        serde_json::from_str(text).or_else(|_| {
            let frame: Envelope<'_> = serde_json::from_str(text)?;
            let raw = || {
                frame
                    .data
                    .map(serde_json::value::RawValue::get)
                    .ok_or_else(|| {
                        <serde_json::Error as serde::de::Error>::custom("missing public payload")
                    })
            };
            match frame.channel.as_ref() {
                "allAssetCtxs" => serde_json::from_str(raw()?).map(Self::Contexts),
                "activeAssetCtx" => serde_json::from_str(raw()?).map(Self::Context),
                "ticker" => serde_json::from_str(raw()?).map(Self::Ticker),
                "l2Book" => serde_json::from_str(raw()?).map(Self::Depth),
                "candle" => frame.data.map(Self::Candle).ok_or_else(|| {
                    <serde_json::Error as serde::de::Error>::custom("missing candle payload")
                }),
                "error" => {
                    let top = serde_json::from_str::<ErrorCode<'_>>(text);
                    if top
                        .as_ref()
                        .is_ok_and(|error| error.code.as_deref() == Some("RISK_RATE_LIMITED"))
                    {
                        top.map(Self::Error)
                    } else if let Some(data) = frame.data {
                        serde_json::from_str(data.get()).map(Self::Error)
                    } else {
                        top.map(Self::Error)
                    }
                }
                _ => Ok(Self::Other),
            }
        })
    }
}
#[derive(Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument {
    #[serde(skip)]
    pub ticker_index: Option<u32>,
    pub index: u32,
    pub name: String,
    pub base_currency: String,
    pub quote_currency: String,
    pub currency_type: String,
    pub delisted: bool,
    pub halt_trading: bool,
    #[serde(deserialize_with = "number_or_zero")]
    pub base_precision: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub price_tick: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub max_leverage: f64,
    #[serde(default)]
    pub only_isolated: bool,
    #[serde(default)]
    pub use_notional_margin: bool,
    #[serde(default)]
    pub max_limit_order_size: Option<f64>,
    #[serde(default)]
    pub max_market_order_size: Option<f64>,
}
impl Instrument {
    pub fn market(&self) -> Option<Market> {
        if self.currency_type != "usdc"
            || self.delisted
            || self.base_precision <= 0.0
            || self.price_tick <= 0.0
            || self.max_leverage < 1.0
        {
            return None;
        }
        Some(Market {
            id: self.index.to_string(),
            symbol: self.name.clone(),
            pretty_symbol: None,
            base: self.base_currency.clone(),
            quote: self.quote_currency.clone(),
            active: !self.halt_trading,
            precision: Precision {
                amount: self.base_precision,
                price: self.price_tick,
            },
            limits: Limits {
                amount: MinMax {
                    min: self.base_precision,
                    max: self.max_limit_order_size.unwrap_or(9_007_199_254_740_991.0),
                },
                leverage: MinMax {
                    min: 1.0,
                    max: self.max_leverage,
                },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: None,
            is_inverse: Some(false),
            is_spot: Some(false),
            is_perpetual: Some(true),
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        })
    }
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Context {
    #[serde(default, deserialize_with = "optional_number")]
    pub funding: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mark_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub oracle_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub open_interest: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mid_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub last_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub prev_day_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number_or_blank")]
    pub day_ntl_vlm: Option<f64>,
    #[serde(default, deserialize_with = "optional_number_or_blank")]
    pub day_base_vlm: Option<f64>,
    #[serde(default)]
    pub next_funding_time: Option<f64>,
}
#[derive(Deserialize)]
pub struct ContextFrame<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub ctx: Context,
}
#[derive(Deserialize)]
pub struct Contexts<'a> {
    #[serde(borrow)]
    pub ctxs: Vec<ContextFrame<'a>>,
}
#[derive(Default, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow {
    #[serde(default)]
    pub instrument_id: u32,
    #[serde(default, deserialize_with = "optional_number")]
    pub last_price: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub prev_day_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub mark_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub oracle_px: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub open_interest: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub day_ntl_vlm: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub coin_vlm: Option<f64>,
    #[serde(default)]
    pub node_ctx: Option<Context>,
}
#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number_or_zero")]
    pub px: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub sz: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow)]
    pub coin: Cow<'a, str>,
    pub time: f64,
    pub levels: [Vec<Level>; 2],
}
#[derive(Deserialize)]
pub struct CandleRow<'a> {
    #[serde(borrow)]
    pub s: Cow<'a, str>,
    #[serde(borrow)]
    pub i: Cow<'a, str>,
    pub t: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub o: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub h: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub l: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub c: f64,
    #[serde(deserialize_with = "number_or_zero")]
    pub v: f64,
}
impl CandleRow<'_> {
    pub fn candle(&self) -> Candle {
        Candle {
            timestamp: self.t / 1000.0,
            open: self.o,
            high: self.h,
            low: self.l,
            close: self.c,
            volume: self.v,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn public_decimal_forms_keep_finite_and_null_contracts() {
        #[derive(Deserialize)]
        struct Decimal(#[serde(deserialize_with = "number_or_zero")] f64);
        for (json, expected) in [
            ("1", 1.0),
            ("-2", -2.0),
            ("1.25", 1.25),
            (r#""1.25""#, 1.25),
            (r#""\u0031.25""#, 1.25),
            (r#""""#, 0.0),
            ("null", 0.0),
        ] {
            assert_eq!(serde_json::from_str::<Decimal>(json).unwrap().0, expected);
        }
        for json in [r#""NaN""#, r#""inf""#, r#""invalid""#, "true", "[]", "{}"] {
            assert!(serde_json::from_str::<Decimal>(json).is_err(), "{json}");
        }
    }

    #[test]
    fn raw_candle_and_error_payloads_survive_reordered_envelopes() {
        for text in [
            r#"{"channel":"candle","data":{"s":"1","i":"1m","t":0,"o":1,"h":2,"l":1,"c":2,"v":3}}"#,
            r#"{"data":{"s":"1","i":"1m","t":0,"o":1,"h":2,"l":1,"c":2,"v":3},"channel":"candle"}"#,
        ] {
            let PublicFrame::Candle(raw) = PublicFrame::decode(text).unwrap() else {
                panic!("candle")
            };
            assert_eq!(
                serde_json::from_str::<CandleRow<'_>>(raw.get()).unwrap().c,
                2.0
            );
        }
        for text in [
            r#"{"channel":"error","data":{"code":"RISK_RATE_LIMITED"}}"#,
            r#"{"data":{"code":"RISK_RATE_LIMITED"},"channel":"error"}"#,
            r#"{"type":"error","code":"RISK_RATE_LIMITED"}"#,
            r#"{"type":"error","code":"RISK_RATE_LIMITED","data":{}}"#,
            r#"{"type":"error","code":"RISK_RATE_LIMITED","data":{"code":123}}"#,
        ] {
            let PublicFrame::Error(error) = PublicFrame::decode(text).unwrap() else {
                panic!("error")
            };
            assert_eq!(error.code.as_deref(), Some("RISK_RATE_LIMITED"));
        }
    }
}
