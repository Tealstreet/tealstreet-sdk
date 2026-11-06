use crate::wire::Number;
use safe_cex_engine::model::*;
use serde::Deserialize;
use serde_json::value::RawValue;

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    pub symbol: &'a str,
    pub base_currency: &'a str,
    pub quote_currency: &'a str,
    pub enable_trading: bool,
    pub base_increment: Number,
    pub price_increment: Number,
    #[serde(default)]
    pub base_min_size: Number,
    #[serde(default)]
    pub base_max_size: Option<Number>,
}
impl Instrument<'_> {
    pub fn market(self) -> Market {
        Market {
            id: self.symbol.into(),
            symbol: self.symbol.into(),
            pretty_symbol: Some(format!("{}{}", self.base_currency, self.quote_currency)),
            base: self.base_currency.into(),
            quote: self.quote_currency.into(),
            active: self.enable_trading,
            precision: Precision {
                amount: self.base_increment.0,
                price: self.price_increment.0,
            },
            limits: Limits {
                amount: MinMax {
                    min: self.base_min_size.0,
                    max: self.base_max_size.map_or(9_007_199_254_740_991.0, |n| n.0),
                },
                leverage: MinMax { min: 1.0, max: 1.0 },
                price: None,
            },
            contract_size: Some(1.0),
            is_expiring: None,
            is_inverse: Some(false),
            is_spot: Some(true),
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        }
    }
}
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Price<'a> {
    #[serde(default)]
    pub symbol: &'a str,
    #[serde(default, alias = "bestBid", alias = "bestBidPrice")]
    pub buy: Option<Number>,
    #[serde(default, alias = "bestAsk", alias = "bestAskPrice")]
    pub sell: Option<Number>,
    #[serde(default, alias = "price", alias = "lastPrice")]
    pub last: Option<Number>,
    #[serde(default)]
    pub change_rate: Option<Number>,
    #[serde(default, alias = "volume", alias = "volume24h")]
    pub vol: Option<Number>,
    #[serde(default, alias = "turnover", alias = "turnover24h")]
    pub vol_value: Option<Number>,
}
impl Price<'_> {
    pub fn apply(&self, t: &mut Ticker) {
        if let Some(n) = self.buy {
            t.bid = n.0;
        }
        if let Some(n) = self.sell {
            t.ask = n.0;
        }
        if let Some(n) = self.last {
            t.last = n.0;
            t.mark = n.0;
            t.index = n.0;
        }
        if let Some(n) = self.change_rate {
            t.percentage = n.0 * 100.0;
        }
        if let Some(n) = self.vol {
            t.volume = n.0;
        }
        if let Some(n) = self.vol_value {
            t.quote_volume = n.0;
        }
    }
    pub fn ticker(&self) -> Ticker {
        let mut t = Ticker {
            id: self.symbol.into(),
            symbol: self.symbol.into(),
            ..Default::default()
        };
        self.apply(&mut t);
        t
    }
}
#[derive(Deserialize)]
pub struct PriceMessage<'a> {
    #[serde(rename = "type")]
    pub kind: &'a str,
    pub topic: &'a str,
    #[serde(default)]
    pub subject: &'a str,
    #[serde(borrow)]
    pub data: Price<'a>,
}
/// The fixed shape of a KuCoin ticker push, `{"topic":T,"type":K,"subject":S,"data":{R}}`,
/// read the way serde reads it as a [`PriceMessage`]. `R` is one flat object: its
/// known fields plain strings, other values plain strings or JSON integers. `None`
/// (escapes, control bytes, null or non-string known values, malformed numbers,
/// whitespace, other keys or order, a repeated field or alias) leaves it to serde.
pub fn scan_price(text: &str) -> Option<PriceMessage<'_>> {
    let (topic, rest) = plain(text.strip_prefix(r#"{"topic":""#)?)?;
    let (kind, rest) = plain(rest.strip_prefix(r#"","type":""#)?)?;
    let (subject, rest) = plain(rest.strip_prefix(r#"","subject":""#)?)?;
    let mut rest = rest.strip_prefix(r#"","data":{""#)?;
    let mut data = Price::default();
    let mut seen = 0u8;
    loop {
        let (key, after) = plain(rest)?;
        let after = after.strip_prefix(r#"":"#)?;
        let (value, after) = match after.strip_prefix('"') {
            Some(string) => {
                let (value, after) = plain(string)?;
                (Some(value), after.strip_prefix('"')?)
            }
            None => (None, integer(after)?),
        };
        let field = match key {
            "symbol" => Some(0),
            "buy" | "bestBid" | "bestBidPrice" => Some(1),
            "sell" | "bestAsk" | "bestAskPrice" => Some(2),
            "last" | "price" | "lastPrice" => Some(3),
            "changeRate" => Some(4),
            "vol" | "volume" | "volume24h" => Some(5),
            "volValue" | "turnover" | "turnover24h" => Some(6),
            _ => None,
        };
        if let Some(bit) = field {
            let value = value?;
            if seen & 1 << bit != 0 {
                return None;
            }
            seen |= 1 << bit;
            if bit == 0 {
                data.symbol = value;
            } else {
                // Number reads blank as 0; a malformed string fails serde too.
                let number = Some(Number(if value.is_empty() {
                    0.0
                } else {
                    safe_cex_engine::num::parse_decimal(value)?
                }));
                match bit {
                    1 => data.buy = number,
                    2 => data.sell = number,
                    3 => data.last = number,
                    4 => data.change_rate = number,
                    5 => data.vol = number,
                    _ => data.vol_value = number,
                }
            }
        }
        if let Some(next) = after.strip_prefix(r#",""#) {
            rest = next;
        } else {
            return (after == "}}").then_some(PriceMessage {
                kind,
                topic,
                subject,
                data,
            });
        }
    }
}

/// A string body up to its closing quote, when it holds no escape or control byte.
fn plain(text: &str) -> Option<(&str, &str)> {
    let end = text
        .bytes()
        .position(|b| b == b'"' || b == b'\\' || b < 0x20)?;
    (text.as_bytes()[end] == b'"').then(|| (&text[..end], &text[end..]))
}

/// The rest after one JSON integer: `-?(0|[1-9][0-9]*)`.
fn integer(text: &str) -> Option<&str> {
    let digits = text.strip_prefix('-').unwrap_or(text);
    let length = match digits.as_bytes().first()? {
        b'0' => 1,
        b'1'..=b'9' => digits.bytes().take_while(u8::is_ascii_digit).count(),
        _ => return None,
    };
    Some(&digits[length..])
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(default, rename = "type")]
    pub kind: &'a str,
    #[serde(default)]
    pub topic: &'a str,
    #[serde(default)]
    pub subject: &'a str,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OrderRow<'a> {
    #[serde(default)]
    pub id: &'a str,
    #[serde(default)]
    pub symbol: &'a str,
    #[serde(default)]
    pub order_id: &'a str,
    #[serde(default)]
    pub client_oid: Option<&'a str>,
    #[serde(default, rename = "type")]
    pub event: &'a str,
    #[serde(default)]
    pub order_type: &'a str,
    #[serde(default)]
    pub order_type_v2: &'a str,
    #[serde(default)]
    pub status: &'a str,
    #[serde(default)]
    pub side: &'a str,
    #[serde(default)]
    pub price: Option<Number>,
    #[serde(default)]
    pub order_price: Option<Number>,
    #[serde(default)]
    pub match_price: Option<Number>,
    #[serde(default)]
    pub size: Option<Number>,
    #[serde(default)]
    pub origin_size: Option<Number>,
    #[serde(default)]
    pub order_size: Option<Number>,
    #[serde(default)]
    pub deal_size: Option<Number>,
    #[serde(default)]
    pub filled_size: Option<Number>,
    #[serde(default)]
    pub remain_size: Option<Number>,
    #[serde(default)]
    pub match_size: Option<Number>,
    #[serde(default)]
    pub trade_id: Option<&'a str>,
    #[serde(default)]
    pub active: bool,
    #[serde(default)]
    pub post_only: bool,
    #[serde(default)]
    pub time_in_force: &'a str,
    #[serde(default)]
    pub last_updated_at: Option<Number>,
    #[serde(default)]
    pub order_time: Option<Number>,
    #[serde(default)]
    pub created_at: Option<Number>,
    #[serde(default)]
    pub ts: Option<Number>,
}
pub fn millis(n: f64) -> f64 {
    if n > 1e17 {
        (n / 1e6).floor()
    } else if n > 1e14 {
        (n / 1e3).floor()
    } else if n < 1e10 {
        n * 1000.0
    } else {
        n
    }
}
impl OrderRow<'_> {
    pub fn id(&self) -> &str {
        if self.id.is_empty() {
            self.order_id
        } else {
            self.id
        }
    }

    pub fn total(&self) -> Option<f64> {
        self.size
            .or(self.origin_size)
            .or(self.order_size)
            .map(|n| n.0)
    }

    pub fn cumulative(&self) -> Option<f64> {
        self.deal_size.or(self.filled_size).map(|n| n.0)
    }

    pub fn order(&self, now: f64, old: Option<&Order>, ws: bool) -> Option<Order> {
        let symbol = if self.symbol.is_empty() {
            old?.symbol.as_str()
        } else {
            self.symbol
        };
        let kind = match if ws {
            if self.order_type.is_empty() {
                self.order_type_v2
            } else {
                self.order_type
            }
        } else {
            self.event
        } {
            "market" => OrderType::Market,
            "limit" => OrderType::Limit,
            _ => old.map_or(OrderType::Limit, |o| o.kind),
        };
        let filled = self
            .cumulative()
            .unwrap_or_else(|| old.map_or(0.0, |o| o.filled));
        let amount = self
            .total()
            .or_else(|| old.map(|o| o.amount))
            .or_else(|| self.remain_size.map(|n| n.0 + filled))
            .unwrap_or(0.0);
        let status = match if ws && matches!(self.event, "canceled" | "cancelled" | "filled") {
            self.event
        } else {
            self.status
        } {
            "canceled" | "cancelled" => OrderStatus::Canceled,
            "done" | "filled" => OrderStatus::Closed,
            "active" | "open" | "new" | "match" => OrderStatus::Open,
            _ => {
                if self.active || ws {
                    OrderStatus::Open
                } else {
                    OrderStatus::Closed
                }
            }
        };
        Some(Order {
            id: self.id().into(),
            parent_id: None,
            status,
            symbol: symbol.into(),
            kind,
            side: match self.side {
                "sell" => OrderSide::Sell,
                "buy" => OrderSide::Buy,
                _ => old.map_or(OrderSide::Buy, |o| o.side),
            },
            price: self
                .price
                .or(self.order_price)
                .or(self.match_price)
                .map_or_else(|| old.map_or(0.0, |o| o.price), |n| n.0),
            amount,
            filled,
            remaining: self.remain_size.map_or((amount - filled).max(0.0), |n| n.0),
            reduce_only: false,
            close: None,
            client_order_id: self
                .client_oid
                .map(str::to_owned)
                .or_else(|| old.and_then(|o| o.client_order_id.clone())),
            time_in_force: Some(if self.post_only {
                TimeInForce::PostOnly
            } else {
                match self.time_in_force {
                    "IOC" => TimeInForce::ImmediateOrCancel,
                    "FOK" => TimeInForce::FillOrKill,
                    _ => old
                        .and_then(|o| o.time_in_force)
                        .unwrap_or(TimeInForce::GoodTillCancel),
                }
            }),
            last_update: self
                .ts
                .or(self.last_updated_at)
                .or(self.order_time)
                .or(self.created_at)
                .map_or(now, |n| millis(n.0)),
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: None,
        })
    }
}
/// What a private list row that failed to decode still says about its identity.
#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase", default)]
pub struct RowIdentity<'a> {
    pub currency: &'a str,
    pub id: &'a str,
    pub order_id: &'a str,
}
#[derive(Deserialize)]
pub struct Account<'a> {
    pub currency: &'a str,
    #[serde(deserialize_with = "required_number")]
    pub available: Number,
    #[serde(deserialize_with = "required_number")]
    pub holds: Number,
    #[serde(deserialize_with = "required_number")]
    pub balance: Number,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Page<'a> {
    #[serde(default)]
    pub total_page: Option<u32>,
    #[serde(borrow)]
    pub items: Vec<&'a RawValue>,
}
#[derive(Deserialize)]
pub struct Symbols<'a> {
    #[serde(borrow)]
    pub symbols: Vec<&'a str>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FillRow<'a> {
    pub symbol: &'a str,
    pub order_id: &'a str,
    pub side: &'a str,
    pub price: Number,
    pub size: Number,
    #[serde(default)]
    pub fee: Number,
    #[serde(default)]
    pub fee_currency: &'a str,
    pub created_at: Number,
}

// Balances must be present: a blank here is not a zero balance.
fn required_number<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Number, D::Error> {
    safe_cex_engine::de::number(d).map(Number)
}

#[cfg(test)]
mod scan_price_tests {
    use super::*;

    const TICKER: &str = r#"{"topic":"/market/ticker:all","type":"message","subject":"ZKC-USDT","data":{"bestAsk":"0.0337","bestAskSize":"13572.63","bestBid":"0.0336","bestBidSize":"5801.46","price":"0.0335","sequence":"241725819","size":"1930","time":1792471145034}}"#;

    type Read = (String, String, String, String, [Option<u64>; 6]);
    fn read(m: &PriceMessage<'_>) -> Read {
        let p = &m.data;
        let bits = [p.buy, p.sell, p.last, p.change_rate, p.vol, p.vol_value]
            .map(|n| n.map(|n| n.0.to_bits()));
        (
            m.kind.into(),
            m.topic.into(),
            m.subject.into(),
            p.symbol.into(),
            bits,
        )
    }

    /// True when the scanner read `text`; it must then read exactly what serde reads.
    fn scan_matches_serde(text: &str) -> bool {
        let Some(scanned) = scan_price(text) else {
            return false;
        };
        let serde = serde_json::from_str::<PriceMessage<'_>>(text)
            .expect("a scanned push is valid for serde");
        assert_eq!(read(&scanned), read(&serde), "{text}");
        true
    }

    #[test]
    fn the_ticker_push_scans_exactly_as_serde_reads_it() {
        assert!(scan_matches_serde(TICKER));
        for (from, to) in [
            (r#""bestAsk":"0.0337""#, r#""bestAsk":"""#),
            (r#""bestBid":"0.0336""#, r#""bestBidPrice":"0.0336""#),
            (r#""price":"0.0335""#, r#""lastPrice":"1e-3""#),
            (
                r#""size":"1930""#,
                r#""changeRate":"-0.0123","volValue":"9","symbol":"X-Y""#,
            ),
            (r#""time":1792471145034"#, r#""time":-0"#),
            (r#""time":1792471145034"#, r#""time":0"#),
            ("ZKC-USDT", "ZKC-USDT\u{e9}"),
        ] {
            let text = TICKER.replace(from, to);
            assert!(scan_matches_serde(&text), "{text}");
        }
    }

    #[test]
    fn anything_off_the_fixed_shape_is_left_to_serde() {
        let off = [
            (r#""price":"0.0335""#, r#""price": "0.0335""#),
            ("0.0335", "0.03BS u0033 5"),
            ("0.0335", "0.03\t35"),
            (r#""price":"0.0335""#, r#""price":null"#),
            (r#""price":"0.0335""#, r#""price":0.0335"#),
            (r#""price":"0.0335""#, r#""price":"abc""#),
            (r#""price":"0.0335""#, r#""price":"1","lastPrice":"2""#),
            (r#""bestBid":"0.0336""#, r#""bestBid":"1","buy":"2""#),
            (r#""time":1792471145034"#, r#""time":01"#),
            (r#""time":1792471145034"#, r#""time":1.5"#),
            (r#""time":1792471145034"#, r#""time":1e3"#),
            (r#""time":1792471145034"#, r#""time":-"#),
            (r#""time":1792471145034"#, r#""time":true"#),
            (r#""time":1792471145034"#, r#""time":{}"#),
            (r#","subject":"ZKC-USDT""#, ""),
            (r#"{"topic""#, r#"{"id":"1","topic""#),
            ("}}", r#"},"x":1}"#),
            (
                r#""type":"message","subject":"ZKC-USDT""#,
                r#""subject":"ZKC-USDT","type":"message""#,
            ),
        ];
        for (case, (from, to)) in off.iter().enumerate() {
            let text = TICKER.replace(from, &to.replace("BS u", "\\u").replace(" 5", "5"));
            assert_ne!(text, TICKER, "case {case} did not apply");
            assert!(scan_price(&text).is_none(), "case {case}: {text:?}");
        }
        assert!(scan_price(&format!("{TICKER}\n")).is_none());
        assert!(scan_price(TICKER.trim_end_matches('}')).is_none());
    }

    /// Deterministic xorshift64 so a failure reproduces from its case number.
    fn next(state: &mut u64) -> u64 {
        *state ^= *state << 13;
        *state ^= *state >> 7;
        *state ^= *state << 17;
        *state
    }

    #[test]
    fn differential_fuzz_never_scans_what_serde_reads_differently() {
        let tokens = [
            "\"",
            "\\",
            ",",
            ":",
            "{",
            "}",
            "[",
            "]",
            " ",
            "\n",
            "\t",
            "\u{1}",
            "0",
            "9",
            ".",
            "-",
            "e",
            "null",
            "true",
            "\"price\"",
            "\"buy\"",
            "\"x\"",
            "\\u0041",
            "\u{e9}",
            "\"\"",
        ];
        let (mut scanned, mut fallback, mut state) = (0, 0, 0x9e37_79b9_7f4a_7c15u64);
        for _ in 0..20_000 {
            let mut text = TICKER.to_owned();
            for _ in 0..=next(&mut state) % 3 {
                let mut at = next(&mut state) as usize % (text.len() + 1);
                while !text.is_char_boundary(at) {
                    at -= 1;
                }
                let token = tokens[next(&mut state) as usize % tokens.len()];
                let mut end = (at + 1 + next(&mut state) as usize % 4).min(text.len());
                while !text.is_char_boundary(end) {
                    end += 1;
                }
                match next(&mut state) % 3 {
                    0 => text.insert_str(at, token),
                    1 => text.replace_range(at..end, ""),
                    _ => text.replace_range(at..end, token),
                }
            }
            if scan_matches_serde(&text) {
                scanned += 1;
            } else {
                fallback += 1;
            }
        }
        // Well-formed pushes with random keys, aliases, order, repeats and values.
        let keys = [
            "symbol",
            "buy",
            "bestBid",
            "bestBidPrice",
            "sell",
            "bestAsk",
            "bestAskPrice",
            "last",
            "price",
            "lastPrice",
            "changeRate",
            "vol",
            "volume",
            "volume24h",
            "volValue",
            "turnover",
            "turnover24h",
            "sequence",
            "size",
            "time",
        ];
        let values = [
            r#""""#,
            r#""1""#,
            r#""0.0337""#,
            r#""-0.5""#,
            r#""1e5""#,
            r#"" 1""#,
            r#""abc""#,
            r#""ZKC-USDT""#,
            "17924",
            "0",
            "-3",
            "null",
        ];
        for _ in 0..5_000 {
            let mut row = Vec::new();
            for _ in 0..next(&mut state) % 8 {
                let key = keys[next(&mut state) as usize % keys.len()];
                let value = values[next(&mut state) as usize % values.len()];
                row.push(format!(r#""{key}":{value}"#));
            }
            let text = format!(
                r#"{{"topic":"/market/ticker:all","type":"message","subject":"S","data":{{{}}}}}"#,
                row.join(",")
            );
            if scan_matches_serde(&text) {
                scanned += 1;
            } else {
                fallback += 1;
            }
        }
        assert!(
            scanned > 2_000 && fallback > 5_000,
            "{scanned} scanned, {fallback} fallback"
        );
    }
}
