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
