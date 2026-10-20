//! Borrowed Bitunix wire records. No intermediate JSON object graph.
use serde::de::{self, Visitor};
use serde::{Deserialize, Deserializer};
use std::{borrow::Cow, fmt};
pub type Str<'a> = Cow<'a, str>;

#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Num(pub f64);
impl<'de> Deserialize<'de> for Num {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Number;
        impl Visitor<'_> for Number {
            type Value = Num;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a number or decimal string")
            }
            fn visit_f64<E: de::Error>(self, v: f64) -> Result<Num, E> {
                Ok(Num(if v.is_finite() { v } else { 0.0 }))
            }
            fn visit_u64<E: de::Error>(self, v: u64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_i64<E: de::Error>(self, v: i64) -> Result<Num, E> {
                Ok(Num(v as f64))
            }
            fn visit_str<E: de::Error>(self, v: &str) -> Result<Num, E> {
                Ok(Num(
                    simple_decimal(v).unwrap_or_else(|| safe_cex_engine::num::parse(v))
                ))
            }
            fn visit_unit<E: de::Error>(self) -> Result<Num, E> {
                Ok(Num(0.0))
            }
        }
        d.deserialize_any(Number)
    }
}

fn simple_decimal(text: &str) -> Option<f64> {
    if text.len() > 64 {
        return None;
    }
    let mut bytes = text.as_bytes();
    let negative = *bytes.first()? == b'-';
    if matches!(bytes[0], b'+' | b'-') {
        bytes = &bytes[1..];
    }
    let mut integer = 0u64;
    let mut significant = 0;
    let mut scale = 0;
    let mut fractional = false;
    let mut has_digits = false;
    for &byte in bytes {
        match byte {
            b'0'..=b'9' => {
                has_digits = true;
                if fractional {
                    scale += 1;
                    if scale > 22 {
                        return None;
                    }
                }
                if integer != 0 || byte != b'0' {
                    significant += 1;
                    if significant > 15 {
                        return None;
                    }
                }
                integer = integer * 10 + u64::from(byte - b'0');
            }
            b'.' if !fractional => fractional = true,
            _ => return None,
        }
    }
    if !has_digits {
        return None;
    }
    // Both operands are exact binary64 integers, so one division rounds as parsing does.
    const POWERS: [f64; 23] = [
        1.0, 1e1, 1e2, 1e3, 1e4, 1e5, 1e6, 1e7, 1e8, 1e9, 1e10, 1e11, 1e12, 1e13, 1e14, 1e15, 1e16,
        1e17, 1e18, 1e19, 1e20, 1e21, 1e22,
    ];
    let value = integer as f64 / POWERS[scale];
    Some(if negative { -value } else { value })
}

#[derive(Deserialize)]
#[serde(untagged)]
pub enum Supported<'a> {
    Bool(bool),
    Number(f64),
    Text(#[serde(borrow)] Str<'a>),
}
impl Supported<'_> {
    pub fn boolean(&self) -> Option<bool> {
        match self {
            Self::Bool(v) => Some(*v),
            Self::Text(v) if v.eq_ignore_ascii_case("true") => Some(true),
            Self::Text(v) if v.eq_ignore_ascii_case("false") => Some(false),
            _ => None,
        }
    }

    pub fn allowed(&self) -> bool {
        match self {
            Self::Bool(v) => *v,
            Self::Number(v) => *v != 0.0,
            Self::Text(v) => !matches!(
                v.trim().to_ascii_lowercase().as_str(),
                "false" | "0" | "no" | "n" | "disabled" | "unsupported"
            ),
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Pair<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    #[serde(borrow)]
    pub base: Str<'a>,
    #[serde(borrow)]
    pub quote: Str<'a>,
    #[serde(borrow)]
    pub symbol_status: Str<'a>,
    pub base_precision: Num,
    pub quote_precision: Num,
    pub max_limit_order_volume: Num,
    #[serde(borrow, default)]
    pub is_api_supported: Option<Supported<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Daily<'a> {
    #[serde(borrow)]
    pub symbol: Str<'a>,
    pub last: Num,
    pub open: Num,
    pub mark_price: Num,
    pub base_vol: Num,
    pub quote_vol: Num,
}
#[derive(Default)]
pub struct Message<'a> {
    pub ch: Str<'a>,
    pub channel: Str<'a>,
    pub symbol: Str<'a>,
    pub op: Str<'a>,
    pub data: Option<&'a serde_json::value::RawValue>,
    pub public: Option<PublicData<'a>>,
}

pub enum PublicData<'a> {
    Tick(Tick),
    Price(Price<'a>),
    Top(Top),
}
impl<'a> PublicData<'a> {
    pub fn parse(channel: &str, text: &'a str) -> Result<Option<Self>, String> {
        match channel {
            "ticker" => serde_json::from_str(text).map(Self::Tick).map(Some),
            "price" => serde_json::from_str(text).map(Self::Price).map(Some),
            "depth_book1" => serde_json::from_str(text).map(Self::Top).map(Some),
            _ => return Ok(None),
        }
        .map_err(|error| error.to_string())
    }
}
impl<'de> Deserialize<'de> for Message<'de> {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(field_identifier, rename_all = "lowercase")]
        enum Field {
            Ch,
            Channel,
            Symbol,
            Op,
            Data,
            #[serde(other)]
            Other,
        }
        #[derive(Deserialize)]
        struct Text<'a>(#[serde(borrow, deserialize_with = "text")] Str<'a>);
        struct Envelope;
        impl<'de> Visitor<'de> for Envelope {
            type Value = Message<'de>;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a Bitunix websocket envelope")
            }
            fn visit_map<A: de::MapAccess<'de>>(self, mut map: A) -> Result<Self::Value, A::Error> {
                let mut message = Message::default();
                let mut seen = 0u8;
                while let Some(field) = map.next_key::<Field>()? {
                    let (bit, name) = match field {
                        Field::Ch => (1, "ch"),
                        Field::Channel => (2, "channel"),
                        Field::Symbol => (4, "symbol"),
                        Field::Op => (8, "op"),
                        Field::Data => (16, "data"),
                        Field::Other => (0, ""),
                    };
                    if seen & bit != 0 {
                        return Err(de::Error::duplicate_field(name));
                    }
                    seen |= bit;
                    match field {
                        Field::Ch => message.ch = map.next_value::<Text<'de>>()?.0,
                        Field::Channel => message.channel = map.next_value::<Text<'de>>()?.0,
                        Field::Symbol => message.symbol = map.next_value::<Text<'de>>()?.0,
                        Field::Op => message.op = map.next_value::<Text<'de>>()?.0,
                        Field::Data => match message.ch.as_ref() {
                            "ticker" => {
                                message.public =
                                    map.next_value::<Option<Tick>>()?.map(PublicData::Tick)
                            }
                            "price" => {
                                message.public = map
                                    .next_value::<Option<Price<'de>>>()?
                                    .map(PublicData::Price)
                            }
                            "depth_book1" => {
                                message.public =
                                    map.next_value::<Option<Top>>()?.map(PublicData::Top)
                            }
                            _ => message.data = map.next_value()?,
                        },
                        Field::Other => {
                            map.next_value::<de::IgnoredAny>()?;
                        }
                    }
                }
                Ok(message)
            }
        }
        d.deserialize_map(Envelope)
    }
}
#[derive(Deserialize)]
pub struct Tick {
    pub la: Num,
    pub o: Num,
    pub b: Num,
    pub q: Num,
}
#[derive(Deserialize)]
pub struct Price<'a> {
    pub mp: Num,
    pub ip: Num,
    pub fr: Num,
    #[serde(borrow, default, deserialize_with = "text")]
    pub ft: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub nft: Str<'a>,
}
#[derive(Deserialize)]
pub struct Depth {
    pub bids: Vec<Level>,
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Top {
    pub b: Vec<Level>,
    pub a: Vec<Level>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Kline {
    pub time: Num,
    pub open: Num,
    pub high: Num,
    pub low: Num,
    pub close: Num,
    #[serde(default)]
    pub volume: Num,
    #[serde(default)]
    pub quote_vol: Num,
}
#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: Num,
    #[serde(borrow, default, deserialize_with = "text")]
    pub msg: Str<'a>,
    #[serde(borrow, default)]
    pub data: Option<&'a serde_json::value::RawValue>,
}
pub fn data(text: &str) -> Result<&str, String> {
    let e: Envelope<'_> = serde_json::from_str(text).map_err(|e| e.to_string())?;
    if e.code.0 != 0.0 {
        return Err(e.msg.into_owned());
    }
    e.data
        .map(|r| r.get())
        .ok_or_else(|| "Bitunix response has no data".into())
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Account<'a> {
    #[serde(borrow, default, deserialize_with = "text")]
    pub margin_coin: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "posMode")]
    pub position_mode: Str<'a>,
    #[serde(default, alias = "availableBalance", alias = "availableMargin")]
    pub available: Option<Num>,
    #[serde(default, alias = "used", alias = "usedMargin")]
    pub margin: Option<Num>,
    #[serde(default, alias = "locked")]
    pub frozen: Option<Num>,
    #[serde(default, alias = "crossUnrealizedPNL")]
    pub cross_unrealized_pnl: Option<Num>,
    #[serde(default, alias = "isolationUnrealizedPNL")]
    pub isolation_unrealized_pnl: Option<Num>,
    #[serde(default, alias = "unrealizedPNL", alias = "upnl")]
    pub unrealized_pnl: Option<Num>,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct PositionRow<'a> {
    #[serde(
        borrow,
        default,
        alias = "instId",
        alias = "contract",
        alias = "instrument"
    )]
    pub symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub position_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "positionSide")]
    pub side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "marginType")]
    pub margin_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "posMode")]
    pub position_mode: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub event: Str<'a>,
    #[serde(default)]
    pub qty: Option<Num>,
    #[serde(default)]
    pub max_qty: Option<Num>,
    #[serde(default)]
    pub size: Option<Num>,
    #[serde(default)]
    pub position_amt: Option<Num>,
    #[serde(default)]
    pub amount: Option<Num>,
    #[serde(default)]
    pub contracts: Option<Num>,
    #[serde(default)]
    pub avg_open_price: Option<Num>,
    #[serde(default)]
    pub entry_price: Option<Num>,
    #[serde(default)]
    pub open_price: Option<Num>,
    #[serde(default)]
    pub avg_price: Option<Num>,
    #[serde(default)]
    pub entry_value: Option<Num>,
    #[serde(default)]
    pub notional: Option<Num>,
    #[serde(default)]
    pub position_value: Option<Num>,
    #[serde(default, alias = "lever")]
    pub leverage: Num,
    #[serde(
        default,
        alias = "unrealizedPNL",
        alias = "unRealizedProfit",
        alias = "upl"
    )]
    pub unrealized_pnl: Num,
    #[serde(default, alias = "realizedPNL")]
    pub realized_pnl: Num,
    #[serde(default, alias = "liquidationPrice", alias = "liqPx")]
    pub liq_price: Num,
    #[serde(default)]
    pub fee: Num,
    #[serde(default)]
    pub funding: Num,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(borrow, default, deserialize_with = "text")]
    pub symbol: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "id")]
    pub order_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub position_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub side: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "type")]
    pub kind: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub client_id: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub effect: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub event: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text", alias = "status")]
    pub order_status: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub tp_stop_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub sl_stop_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub tp_order_type: Str<'a>,
    #[serde(borrow, default, deserialize_with = "text")]
    pub sl_order_type: Str<'a>,
    #[serde(default)]
    pub price: Num,
    #[serde(default)]
    pub qty: Num,
    #[serde(default)]
    pub trade_qty: Num,
    #[serde(default)]
    pub deal_amount: Option<Num>,
    #[serde(default, alias = "avgPrice")]
    pub average_price: Num,
    #[serde(borrow, default, deserialize_with = "text")]
    pub mtime: Str<'a>,
    #[serde(default)]
    pub tp_price: Option<Num>,
    #[serde(default)]
    pub sl_price: Option<Num>,
    #[serde(default)]
    pub tp_qty: Num,
    #[serde(default)]
    pub sl_qty: Num,
    #[serde(default)]
    pub tp_order_price: Num,
    #[serde(default)]
    pub sl_order_price: Num,
    #[serde(borrow, default, alias = "reductionOnly")]
    pub reduce_only: Option<Supported<'a>>,
}

impl PositionRow<'_> {
    pub fn quantity(&self) -> f64 {
        self.qty
            .or(self.max_qty)
            .or(self.size)
            .or(self.position_amt)
            .or(self.amount)
            .or(self.contracts)
            .unwrap_or_default()
            .0
    }
    pub fn entry(&self) -> f64 {
        self.avg_open_price
            .or(self.entry_price)
            .or(self.open_price)
            .or(self.avg_price)
            .unwrap_or_default()
            .0
    }
    pub fn notional(&self) -> f64 {
        self.entry_value
            .or(self.notional)
            .or(self.position_value)
            .unwrap_or_default()
            .0
    }
}

pub(crate) fn text<'de, D: Deserializer<'de>>(d: D) -> Result<Str<'de>, D::Error> {
    struct Text;
    impl<'de> Visitor<'de> for Text {
        type Value = Str<'de>;
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("text or null")
        }
        fn visit_borrowed_str<E: de::Error>(self, v: &'de str) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(v))
        }
        fn visit_str<E: de::Error>(self, v: &str) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.into()))
        }
        fn visit_unit<E: de::Error>(self) -> Result<Self::Value, E> {
            Ok(Cow::Borrowed(""))
        }
        fn visit_u64<E: de::Error>(self, v: u64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.to_string()))
        }
        fn visit_i64<E: de::Error>(self, v: i64) -> Result<Self::Value, E> {
            Ok(Cow::Owned(v.to_string()))
        }
    }
    d.deserialize_any(Text)
}

pub fn time_ms(s: &str) -> f64 {
    s.parse::<f64>()
        .ok()
        .filter(|n| n.is_finite())
        .or_else(|| rfc3339_ms(s).map(|t| t as f64))
        .unwrap_or(0.0)
}

fn rfc3339_ms(s: &str) -> Option<i64> {
    fn digits(bytes: &[u8]) -> Option<i64> {
        bytes.iter().try_fold(0, |value, byte| {
            byte.is_ascii_digit()
                .then(|| value * 10 + i64::from(byte - b'0'))
        })
    }
    let b = s.as_bytes();
    if b.len() < 20
        || b[4] != b'-'
        || b[7] != b'-'
        || !matches!(b[10], b'T' | b't')
        || b[13] != b':'
        || b[16] != b':'
    {
        return None;
    }
    let year = digits(&b[..4])?;
    let month = digits(&b[5..7])?;
    let day = digits(&b[8..10])?;
    let hour = digits(&b[11..13])?;
    let minute = digits(&b[14..16])?;
    let second = digits(&b[17..19])?;
    let leap = year % 4 == 0 && (year % 100 != 0 || year % 400 == 0);
    let days_in_month = match month {
        2 => {
            if leap {
                29
            } else {
                28
            }
        }
        4 | 6 | 9 | 11 => 30,
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        _ => return None,
    };
    if !(1..=days_in_month).contains(&day) || hour > 23 || minute > 59 || second > 59 {
        return None;
    }
    let mut index = 19;
    let mut millis = 0;
    if b[index] == b'.' {
        index += 1;
        let start = index;
        while b.get(index).is_some_and(u8::is_ascii_digit) {
            if index - start < 3 {
                millis += i64::from(b[index] - b'0') * [100, 10, 1][index - start];
            }
            index += 1;
        }
        if index == start {
            return None;
        }
    }
    let offset = match b.get(index..)? {
        [b'Z' | b'z'] => 0,
        [sign @ (b'+' | b'-'), h1, h2, b':', m1, m2] => {
            let hours = digits(&[*h1, *h2])?;
            let minutes = digits(&[*m1, *m2])?;
            if hours > 23 || minutes > 59 {
                return None;
            }
            (hours * 60 + minutes) * if *sign == b'+' { 60 } else { -60 }
        }
        _ => return None,
    };
    // Gregorian civil date to days since 1970-01-01, including pre-epoch dates.
    let adjusted_year = year - i64::from(month <= 2);
    let era = adjusted_year.div_euclid(400);
    let year_of_era = adjusted_year - era * 400;
    let shifted_month = month + if month > 2 { -3 } else { 9 };
    let day_of_year = (153 * shifted_month + 2) / 5 + day - 1;
    let days = era * 146097 + year_of_era * 365 + year_of_era / 4 - year_of_era / 100 + day_of_year
        - 719468;
    Some((days * 86400 + hour * 3600 + minute * 60 + second - offset) * 1000 + millis)
}

/// Required quantities on mutation recovery must never default malformed input to zero.
#[derive(Deserialize)]
#[serde(transparent)]
pub struct StrictNum(#[serde(deserialize_with = "strict_number")] pub f64);
fn strict_number<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Numeric<'a> {
        Number(f64),
        Text(#[serde(borrow)] Str<'a>),
    }
    let value = match Numeric::deserialize(d)? {
        Numeric::Number(v) => v,
        Numeric::Text(s) => s.parse::<f64>().map_err(de::Error::custom)?,
    };
    if value.is_finite() {
        Ok(value)
    } else {
        Err(de::Error::custom("nonfinite quantity"))
    }
}

/// The legacy feed appends unused columns after price and quantity.
pub struct Level(pub f64, pub f64);
impl<'de> Deserialize<'de> for Level {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        struct Pair;
        impl<'de> Visitor<'de> for Pair {
            type Value = Level;
            fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
                f.write_str("a price and quantity level")
            }
            fn visit_seq<A: de::SeqAccess<'de>>(self, mut a: A) -> Result<Level, A::Error> {
                let price: Num = a
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("missing price"))?;
                let amount: Num = a
                    .next_element()?
                    .ok_or_else(|| de::Error::custom("missing amount"))?;
                while a.next_element::<de::IgnoredAny>()?.is_some() {}
                Ok(Level(price.0, amount.0))
            }
        }
        d.deserialize_seq(Pair)
    }
}

#[cfg(test)]
mod tests {
    use super::{Message, Num, PublicData, rfc3339_ms, simple_decimal, time_ms};

    #[test]
    fn websocket_data_decodes_directly_when_channel_precedes_it() {
        for text in [
            r#"{"ch":"ticker","symbol":"BTCUSDT","data":{"la":"7","o":"6","b":"5","q":"4"}}"#,
            r#"{"ch":"ticker","data":{"la":"7","o":"6","b":"5","q":"4"},"symbol":"BTCUSDT"}"#,
        ] {
            let message: Message<'_> = serde_json::from_str(text).unwrap();
            assert!(message.data.is_none());
            assert!(matches!(message.public, Some(PublicData::Tick(ref tick)) if tick.la.0 == 7.0));
            assert!(matches!(
                message.symbol,
                std::borrow::Cow::Borrowed("BTCUSDT")
            ));
        }
        let message: Message<'_> = serde_json::from_str(
            r#"{"data":{"la":"7","o":"6","b":"5","q":"4"},"symbol":"BTCUSDT","ch":"ticker"}"#,
        )
        .unwrap();
        assert!(message.public.is_none());
        assert!(
            matches!(PublicData::parse(&message.ch, message.data.unwrap().get()).unwrap(), Some(PublicData::Tick(tick)) if tick.la.0 == 7.0)
        );
        let private: Message<'_> =
            serde_json::from_str(r#"{"ch":"order","data":{"orderId":"one"}}"#).unwrap();
        assert!(private.public.is_none());
        assert_eq!(private.data.unwrap().get(), r#"{"orderId":"one"}"#);
    }

    #[test]
    fn decimal_fast_path_matches_std_bits_for_prices_fees_and_signed_zero() {
        for value in [
            "0",
            "-0",
            "+0",
            "-0.000",
            "0.000001",
            "-0.000001",
            ".1",
            "1.",
            "+.1",
            "-.1",
            "96001.25",
            "-0.013461",
            "999999999999999",
            "99999999999999.9",
            "0.0000000000000000000001",
            "000000000000000000000000000000001.000000",
        ] {
            assert_eq!(
                simple_decimal(value).unwrap().to_bits(),
                value.parse::<f64>().unwrap().to_bits(),
                "{value}",
            );
        }
    }

    #[test]
    fn decimal_fast_path_sweep_matches_correctly_rounded_std_parser() {
        for number in 0..10_000u64 {
            for scale in [0, 1, 2, 3, 4, 6, 8, 12, 18, 22] {
                let value = if scale == 0 {
                    number.to_string()
                } else {
                    format!("0.{number:0scale$}")
                };
                for sign in ["", "-"] {
                    let text = format!("{sign}{value}");
                    assert_eq!(
                        simple_decimal(&text).unwrap().to_bits(),
                        text.parse::<f64>().unwrap().to_bits(),
                        "{text}",
                    );
                }
            }
        }
        for integer in [1, 97, 95_000, 999_999_999] {
            for fraction in [1, 111, 500, 999] {
                let text = format!("{integer}.{fraction:03}");
                assert_eq!(
                    simple_decimal(&text).unwrap().to_bits(),
                    text.parse::<f64>().unwrap().to_bits(),
                    "{text}",
                );
            }
        }
    }

    #[test]
    fn uncommon_and_invalid_numeric_strings_keep_existing_fallback_semantics() {
        for value in [
            "",
            ".",
            "-",
            "+",
            "-.",
            "1.2.3",
            " 1",
            "1 ",
            "é",
            "1e3",
            "-1E-8",
            "1e999",
            "NaN",
            "inf",
            "-inf",
            "1000000000000000",
            "1234567890123456",
            "0.00000000000000000000001",
            "00000000000000000000000000000000000000000000000000000000000000001",
        ] {
            assert!(simple_decimal(value).is_none(), "{value:?}");
            let wire = serde_json::to_string(value).unwrap();
            let actual: Num = serde_json::from_str(&wire).unwrap();
            assert_eq!(
                actual.0.to_bits(),
                safe_cex_engine::num::parse(value).to_bits(),
                "{value:?}",
            );
        }
    }

    #[test]
    fn timestamp_offsets_and_submillisecond_precision_match_utc() {
        let expected = 1743869139369;
        for value in [
            "2025-04-05T16:05:39.369077000Z",
            "2025-04-06T00:05:39.369+08:00",
            "2025-04-05T11:05:39.369-05:00",
            "2025-04-05t16:05:39.369z",
        ] {
            assert_eq!(rfc3339_ms(value), Some(expected), "{value}");
        }
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00.1Z"), Some(100));
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00.12Z"), Some(120));
        assert_eq!(rfc3339_ms("1969-12-31T23:59:59.999999999Z"), Some(-1));
        assert_eq!(time_ms("1743869139369"), expected as f64);
        assert_eq!(time_ms("NaN"), 0.0);
    }

    #[test]
    fn gregorian_calendar_handles_leap_centuries_and_pre_epoch_dates() {
        assert_eq!(rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
        assert_eq!(rfc3339_ms("0001-01-01T00:00:00Z"), Some(-62135596800000));
        assert_eq!(rfc3339_ms("2000-02-29T00:00:00Z"), Some(951782400000));
        assert_eq!(rfc3339_ms("2024-02-29T00:00:00Z"), Some(1709164800000));
        assert!(rfc3339_ms("1900-02-29T00:00:00Z").is_none());
        assert!(rfc3339_ms("2100-02-29T00:00:00Z").is_none());
    }

    #[test]
    fn malformed_timestamps_are_rejected_without_panicking() {
        for value in [
            "",
            "é",
            "2025-04-05",
            "2025-04-05T16:05:39",
            "2025-04-05T16:05:39.Z",
            "2025-04-05T16:05:39.1",
            "2025-04-05T16:05:39Zjunk",
            "2025-04-05T16:05:39+0800",
            "2025-04-05T16:05:39+24:00",
            "2025-04-05T16:05:39-00:60",
            "2025-04-05T24:00:00Z",
            "2025-04-05T16:60:00Z",
            "2025-04-05T16:05:60Z",
            "2025-00-05T16:05:39Z",
            "2025-13-05T16:05:39Z",
            "2025-04-00T16:05:39Z",
            "2025-04-31T16:05:39Z",
            "2025-02-29T16:05:39Z",
            "2025-04-05T16:05:39.éZ",
            "2025-é-05T16:05:39Z",
            "2025-04-05T16:05:39Z\n",
        ] {
            assert_eq!(rfc3339_ms(value), None, "{value:?}");
        }
    }
}
