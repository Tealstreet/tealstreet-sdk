use serde::Deserialize;
use std::borrow::Cow;

#[derive(Clone, Copy, Debug, Default)]
pub struct Number(pub Option<f64>);

impl Number {
    pub fn get(self) -> f64 {
        self.0.unwrap_or(0.0)
    }
    pub fn or(self, fallback: f64) -> f64 {
        self.0.unwrap_or(fallback)
    }
}

impl<'de> Deserialize<'de> for Number {
    fn deserialize<D: serde::Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        #[serde(untagged)]
        enum Wire<'a> {
            Text(#[serde(borrow)] Cow<'a, str>),
            Numeric(f64),
            Null,
        }
        let value = match Wire::deserialize(d)? {
            Wire::Text(text) => text.parse::<f64>().ok(),
            Wire::Numeric(value) => Some(value),
            Wire::Null => None,
        };
        Ok(Self(value.filter(|v| v.is_finite())))
    }
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Contract<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(borrow)]
    pub base_coin: Cow<'a, str>,
    #[serde(borrow)]
    pub quote_coin: Cow<'a, str>,
    #[serde(borrow)]
    pub symbol_status: Cow<'a, str>,
    pub price_place: Number,
    pub price_end_step: Number,
    pub size_multiplier: Number,
    #[serde(default)]
    pub min_trade_num: Number,
    #[serde(default)]
    pub max_order_qty: Number,
    #[serde(default)]
    pub max_market_order_qty: Number,
    #[serde(default)]
    pub max_trade_num: Number,
    #[serde(default)]
    pub min_lever: Number,
    #[serde(default)]
    pub max_lever: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct TickerRow<'a> {
    #[serde(default, borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, borrow)]
    pub inst_id: Cow<'a, str>,
    #[serde(default, alias = "bidPrice", alias = "bid")]
    pub bid_pr: Number,
    #[serde(default, alias = "askPrice", alias = "ask")]
    pub ask_pr: Number,
    #[serde(default, alias = "lastPrice", alias = "last")]
    pub last_pr: Number,
    #[serde(default, alias = "markPr", alias = "mark")]
    pub mark_price: Number,
    #[serde(default, alias = "indexPr", alias = "index")]
    pub index_price: Number,
    #[serde(default, alias = "change")]
    pub change24h: Number,
    #[serde(default)]
    pub change_utc24h: Number,
    #[serde(default)]
    pub funding_rate: Number,
    #[serde(default, alias = "fundingTime")]
    pub next_funding_time: Number,
    #[serde(default, alias = "volume")]
    pub base_volume: Number,
    #[serde(default)]
    pub quote_volume: Number,
}

#[derive(Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Argument<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, borrow)]
    pub inst_id: Cow<'a, str>,
}

#[derive(Deserialize)]
pub struct Message<'a, T> {
    #[serde(default, borrow)]
    pub arg: Argument<'a>,
    #[serde(default, borrow)]
    pub action: Cow<'a, str>,
    #[serde(default, borrow)]
    pub event: Cow<'a, str>,

    #[serde(default, borrow)]
    pub msg: Cow<'a, str>,
    pub data: Option<Vec<T>>,
}

#[derive(Deserialize, Default)]
pub struct Depth<'a> {
    #[serde(default, borrow)]
    pub bids: Option<Vec<[Cow<'a, str>; 2]>>,
    #[serde(default, borrow)]
    pub asks: Option<Vec<[Cow<'a, str>; 2]>>,
    pub checksum: Option<i32>,
    #[serde(default, deserialize_with = "sequence")]
    pub seq: Option<i64>,
    #[serde(default, deserialize_with = "sequence")]
    pub pseq: Option<i64>,
}

pub type CandleRow = [Number; 7];

fn sequence<'de, D: serde::Deserializer<'de>>(d: D) -> Result<Option<i64>, D::Error> {
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum Sequence<'a> {
        Text(#[serde(borrow)] Cow<'a, str>),
        Integer(i64),
    }
    Option::<Sequence>::deserialize(d)?
        .map(|value| match value {
            Sequence::Text(text) => text.parse().map_err(serde::de::Error::custom),
            Sequence::Integer(value) => Ok(value),
        })
        .transpose()
}
