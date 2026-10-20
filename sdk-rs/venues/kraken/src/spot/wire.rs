use crate::wire::{number, optional_number};
use serde::{
    Deserialize, Deserializer,
    de::{MapAccess, Visitor},
};
use serde_json::value::RawValue;
use std::{borrow::Cow, fmt, marker::PhantomData};

#[derive(Deserialize)]
pub struct Envelope<'a> {
    pub error: Vec<Cow<'a, str>>,
    #[serde(borrow)]
    pub result: Option<&'a RawValue>,
}
pub fn result(response: safe_cex_engine::Response<'_>) -> Result<&str, String> {
    let e: Envelope<'_> =
        serde_json::from_str(response.body).map_err(|_| "Kraken Spot invalid response")?;
    if !(200..300).contains(&response.status) || !e.error.is_empty() {
        return Err(if e.error.is_empty() {
            format!("Kraken Spot HTTP {}", response.status)
        } else {
            format!("Kraken Spot: {}", e.error.join("; "))
        });
    }
    e.result
        .map(RawValue::get)
        .ok_or_else(|| "Kraken Spot missing result".into())
}
// Object-keyed Kraken catalogs are decoded one row at a time, without a wire tree.
pub fn map_each<'de, T: Deserialize<'de>>(
    text: &'de str,
    mut each: impl FnMut(Cow<'de, str>, T),
) -> serde_json::Result<()> {
    struct Rows<'a, T, F>(&'a mut F, PhantomData<T>);
    impl<'de, T: Deserialize<'de>, F: FnMut(Cow<'de, str>, T)> Visitor<'de> for Rows<'_, T, F> {
        type Value = ();
        fn expecting(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            f.write_str("keyed Kraken records")
        }
        fn visit_map<A: MapAccess<'de>>(self, mut map: A) -> Result<(), A::Error> {
            while let Some(key) = map.next_key::<Cow<'de, str>>()? {
                (self.0)(key, map.next_value::<T>()?);
            }
            Ok(())
        }
    }
    let mut de = serde_json::Deserializer::from_str(text);
    de.deserialize_map(Rows(&mut each, PhantomData::<T>))?;
    de.end()
}
#[derive(Deserialize)]
pub struct Pair<'a> {
    #[serde(default, borrow)]
    pub wsname: Cow<'a, str>,
    #[serde(default, borrow)]
    pub altname: Cow<'a, str>,
    #[serde(default, borrow)]
    pub status: Cow<'a, str>,
    #[serde(default)]
    pub pair_decimals: Option<u32>,
    #[serde(default)]
    pub lot_decimals: Option<u32>,
    #[serde(default, deserialize_with = "number")]
    pub ordermin: f64,
}
#[derive(Deserialize)]
pub struct RestTicker {
    pub a: Vec<String>,
    pub b: Vec<String>,
    pub c: Vec<String>,
    pub v: Vec<String>,
    pub o: String,
}
#[derive(Deserialize)]
pub struct Ws<'a> {
    #[serde(default, borrow)]
    pub channel: Cow<'a, str>,
    #[serde(default, rename = "type", borrow)]
    pub kind: Cow<'a, str>,
    #[serde(default, borrow)]
    pub data: Option<&'a RawValue>,
    #[serde(default)]
    pub success: Option<bool>,
    #[serde(default, borrow)]
    pub error: Cow<'a, str>,
}
#[derive(Deserialize)]
pub struct WsTicker<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default, deserialize_with = "optional_number")]
    pub bid: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub ask: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub last: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub volume: Option<f64>,
    #[serde(default, deserialize_with = "optional_number")]
    pub change_pct: Option<f64>,
}
#[derive(Deserialize)]
pub struct Level {
    #[serde(deserialize_with = "number")]
    pub price: f64,
    #[serde(deserialize_with = "number")]
    pub qty: f64,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    #[serde(default)]
    pub bids: Vec<Level>,
    #[serde(default)]
    pub asks: Vec<Level>,
}
#[derive(Deserialize)]
pub struct Ohlc<'a> {
    #[serde(borrow)]
    pub symbol: Cow<'a, str>,
    pub interval: u32,
    #[serde(borrow)]
    pub interval_begin: Cow<'a, str>,
    #[serde(deserialize_with = "number")]
    pub open: f64,
    #[serde(deserialize_with = "number")]
    pub high: f64,
    #[serde(deserialize_with = "number")]
    pub low: f64,
    #[serde(deserialize_with = "number")]
    pub close: f64,
    #[serde(deserialize_with = "number")]
    pub volume: f64,
}
pub fn interval(tf: &str) -> Option<u32> {
    Some(match tf {
        "1m" | "3m" => 1,
        "5m" => 5,
        "15m" => 15,
        "30m" => 30,
        "1h" | "2h" => 60,
        "4h" | "6h" | "12h" => 240,
        "1d" => 1440,
        "1w" => 10080,
        _ => return None,
    })
}
pub fn asset(a: &str) -> &str {
    match a {
        "XBT" | "XXBT" => "BTC",
        "XETH" => "ETH",
        "XXRP" => "XRP",
        "XLTC" => "LTC",
        "XXLM" => "XLM",
        "XXMR" => "XMR",
        "XZEC" => "ZEC",
        "XREP" => "REP",
        "XETC" => "ETC",
        "XDAO" => "DAO",
        "XMLN" => "MLN",
        "ZUSD" => "USD",
        "ZEUR" => "EUR",
        "ZGBP" => "GBP",
        "ZJPY" => "JPY",
        "ZCAD" => "CAD",
        "ZAUD" => "AUD",
        _ => a,
    }
}
