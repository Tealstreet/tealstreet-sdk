use safe_cex_engine::model::{Candle, Limits, Market, MinMax, Precision, Ticker};
use serde::{Deserialize, Deserializer};
use serde_json::value::RawValue;

#[derive(Clone, Copy, Debug, Default)]
pub struct Number(pub f64);
impl Number {
    pub fn get(self) -> f64 {
        self.0
    }
}
// KuCoin sends "" or null for values that do not apply; both read as 0.
impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        safe_cex_engine::de::number_or_zero(d).map(Self)
    }
}

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub code: &'a str,
    #[serde(default)]
    pub msg: &'a str,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}

#[derive(Deserialize)]
pub struct Message<'a> {
    #[serde(rename = "type", default)]
    pub kind: &'a str,
    #[serde(default)]
    pub topic: &'a str,
    #[serde(borrow)]
    pub data: Option<&'a RawValue>,
}

/// The identity of a contract row that failed to decode.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InstrumentKey<'a> {
    #[serde(borrow)]
    pub symbol: std::borrow::Cow<'a, str>,
    #[serde(borrow, default)]
    pub settle_currency: Option<std::borrow::Cow<'a, str>>,
    #[serde(borrow, default)]
    pub quote_currency: Option<std::borrow::Cow<'a, str>>,
}
impl InstrumentKey<'_> {
    /// Unless its settlement is readable and not USDT, the row may be one this venue lists.
    pub fn in_scope(&self) -> bool {
        self.settle_currency
            .as_deref()
            .filter(|s| !s.is_empty())
            .or(self.quote_currency.as_deref())
            .is_none_or(|quote| quote == "USDT")
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Instrument<'a> {
    pub symbol: &'a str,
    #[serde(default)]
    pub settle_currency: &'a str,
    #[serde(default)]
    pub quote_currency: &'a str,
    #[serde(default)]
    pub base_currency: &'a str,
    #[serde(default)]
    pub root_symbol: &'a str,
    pub status: &'a str,
    #[serde(default)]
    pub lot_size: Number,
    #[serde(default)]
    pub tick_size: Number,
    #[serde(default)]
    pub min_order_qty: Number,
    #[serde(default)]
    pub max_order_qty: Number,
    #[serde(default)]
    pub max_leverage: Number,
    #[serde(default)]
    pub multiplier: Number,
    #[serde(default)]
    pub expire_date: Option<&'a RawValue>,
    #[serde(default)]
    pub is_inverse: bool,
}
impl Instrument<'_> {
    pub fn market(&self) -> Option<Market> {
        let quote = if self.settle_currency.is_empty() {
            self.quote_currency
        } else {
            self.settle_currency
        };
        let expiring = self
            .expire_date
            .is_some_and(|v| !matches!(v.get(), "null" | "0" | "\"\""));
        if quote != "USDT" || expiring {
            return None;
        }
        let base = if self.base_currency.is_empty() {
            self.root_symbol
        } else {
            self.base_currency
        };
        let lot = if self.lot_size.0 > 0.0 {
            self.lot_size.0
        } else {
            1.0
        };
        Some(Market {
            id: self.symbol.into(),
            symbol: crate::internal_symbol(self.symbol),
            pretty_symbol: None,
            base: if base == "XBT" {
                "BTC".into()
            } else {
                base.into()
            },
            quote: quote.into(),
            active: matches!(self.status, "Open" | "ONLINE"),
            precision: Precision {
                amount: lot,
                price: if self.tick_size.0 > 0.0 {
                    self.tick_size.0
                } else {
                    0.01
                },
            },
            limits: Limits {
                amount: MinMax {
                    min: if self.min_order_qty.0 > 0.0 {
                        self.min_order_qty.0
                    } else {
                        lot
                    },
                    max: if self.max_order_qty.0 > 0.0 {
                        self.max_order_qty.0
                    } else {
                        f64::INFINITY
                    },
                },
                leverage: MinMax {
                    min: 1.0,
                    max: if self.max_leverage.0 > 0.0 {
                        self.max_leverage.0
                    } else {
                        20.0
                    },
                },
                price: None,
            },
            contract_size: Some(if self.multiplier.0 > 0.0 {
                self.multiplier.0
            } else {
                1.0
            }),
            is_expiring: Some(false),
            is_inverse: Some(self.is_inverse),
            is_spot: None,
            is_perpetual: None,
            is_read_only: None,
            taker_fee: None,
            maker_fee: None,
        })
    }
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow<'a> {
    pub symbol: &'a str,
    #[serde(default)]
    pub price: Number,
    #[serde(default)]
    pub best_bid_price: Option<Number>,
    #[serde(default)]
    pub best_ask_price: Option<Number>,
    #[serde(default)]
    pub mark_price: Option<Number>,
    #[serde(default)]
    pub index_price: Option<Number>,
    #[serde(default)]
    pub last_price: Option<Number>,
    #[serde(default)]
    pub price_chg_pct: Option<Number>,
    #[serde(default)]
    pub funding_rate: Option<Number>,
    #[serde(default)]
    pub volume: Option<Number>,
    #[serde(default)]
    pub turnover: Option<Number>,
    #[serde(default, rename = "volumeOf24h")]
    pub volume_of_24h: Option<Number>,
    #[serde(default, rename = "volume24h")]
    pub volume_24h: Option<Number>,
    #[serde(default, rename = "turnoverOf24h")]
    pub turnover_of_24h: Option<Number>,
    #[serde(default, rename = "turnover24h")]
    pub turnover_24h: Option<Number>,
    #[serde(default)]
    pub size: Number,
}
impl TickerRow<'_> {
    pub fn ticker(&self, symbol: String) -> Ticker {
        Ticker {
            id: self.symbol.into(),
            symbol,
            bid: self.best_bid_price.unwrap_or_default().0,
            ask: self.best_ask_price.unwrap_or_default().0,
            last: self.price.0,
            mark: self.mark_price.unwrap_or(self.price).0,
            index: self.index_price.unwrap_or(self.price).0,
            volume: self
                .volume_of_24h
                .or(self.volume_24h)
                .unwrap_or(self.size)
                .0,
            quote_volume: self
                .turnover_of_24h
                .or(self.turnover_24h)
                .unwrap_or_default()
                .0,
            ..Default::default()
        }
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bullet<'a> {
    pub token: &'a str,
    #[serde(borrow)]
    pub instance_servers: Vec<Server<'a>>,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Server<'a> {
    pub endpoint: &'a str,
    pub ping_interval: f64,
    pub ping_timeout: f64,
}

#[derive(Deserialize)]
pub struct Snapshot {
    pub sequence: u64,
    pub bids: Vec<(Number, Number)>,
    pub asks: Vec<(Number, Number)>,
}
#[derive(Deserialize)]
pub struct Increment<'a> {
    pub sequence: u64,
    pub change: &'a str,
}

pub fn candle(row: &[Number], rest: bool) -> Option<Candle> {
    if row.len() < 6 {
        return None;
    }
    Some(Candle {
        timestamp: row[0].0 / if rest { 1000.0 } else { 1.0 },
        open: row[1].0,
        high: row[if rest { 2 } else { 3 }].0,
        low: row[if rest { 3 } else { 4 }].0,
        close: row[if rest { 4 } else { 2 }].0,
        volume: row[5].0,
    })
}

/// Refused before any request was sent.
pub(crate) fn not_sent(message: impl Into<String>) -> safe_cex_engine::VenueError {
    safe_cex_engine::VenueError::new(message).outcome(safe_cex_engine::Outcome::NotSent)
}

/// A failed private write answer. 408/5xx, transport loss (status 0) and
/// malformed 2xx answers do not prove a refusal; documented codes are
/// classified and others stay unset.
pub(crate) fn write_failure(
    status: u16,
    body: &str,
    message: String,
) -> safe_cex_engine::VenueError {
    use safe_cex_engine::{ErrorKind, Outcome, VenueError};
    let error = VenueError::new(message);
    let rejected = |error: VenueError, kind| error.kind(kind).outcome(Outcome::Rejected);
    if status == 0 {
        return error.kind(ErrorKind::Network).outcome(Outcome::Unknown);
    }
    let envelope = serde_json::from_str::<Envelope<'_>>(body).ok();
    let error = match envelope.as_ref() {
        Some(e) if !e.code.is_empty() => error.venue_code(e.code),
        _ => error,
    };
    let error = match envelope.as_ref() {
        Some(e) if !e.msg.is_empty() => error.venue_message(e.msg),
        _ => error,
    };
    match status {
        200..=299 => {}
        401 => return rejected(error, ErrorKind::AuthInvalid),
        403 => return rejected(error, ErrorKind::AuthPermission),
        429 => return rejected(error, ErrorKind::RateLimited),
        408 => return error.outcome(Outcome::Unknown),
        400..=499 => return rejected(error, ErrorKind::Rejected),
        _ => return error.outcome(Outcome::Unknown),
    }
    let Some(code) = envelope.map(|e| e.code).filter(|code| *code != "200000") else {
        return error.outcome(Outcome::Unknown);
    };
    match code {
        "400001" | "400002" | "400003" | "400004" | "400005" => {
            rejected(error, ErrorKind::AuthInvalid)
        }
        "400006" | "400007" | "411100" => rejected(error, ErrorKind::AuthPermission),
        "429000" => rejected(error, ErrorKind::RateLimited),
        "400100" | "200004" => rejected(error, ErrorKind::Rejected),
        code if code.starts_with("3000") && code.len() == 6 => rejected(error, ErrorKind::Rejected),
        "500000" => error.outcome(Outcome::Unknown),
        _ => error,
    }
}
