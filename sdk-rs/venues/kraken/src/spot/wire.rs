use crate::wire::{number, optional_number};
use safe_cex_engine::model::Interval;
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
pub struct PairName<'a> {
    #[serde(default, borrow)]
    pub wsname: Cow<'a, str>,
}
/// The base a rejected pair would have valued in USD, read from its key or wsname.
pub fn rejected_usd_route(key: &str, wsname: &str) -> Vec<String> {
    let usd = |q: &str| matches!(asset(q), "USD" | "USDT" | "USDC");
    let mut bases = vec![];
    if let Some(base) = ["ZUSD", "USDT", "USDC", "USD"]
        .iter()
        .find_map(|q| key.strip_suffix(q))
        .filter(|b| !b.is_empty())
    {
        bases.push(asset(base).to_owned());
    }
    if let Some((base, quote)) = wsname.split_once('/') {
        let (base, quote) = (base.trim(), quote.trim().trim_start_matches('/'));
        if !base.is_empty() && usd(quote) {
            bases.push(asset(base).to_owned());
        }
    }
    bases
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
    pub method: Cow<'a, str>,
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
pub struct Decimal {
    pub value: f64,
    pub digits: String,
}
impl<'de> Deserialize<'de> for Decimal {
    fn deserialize<D: Deserializer<'de>>(d: D) -> Result<Self, D::Error> {
        let raw = Box::<RawValue>::deserialize(d)?;
        let token = raw.get().trim_matches('"');
        let value = token
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && *v >= 0.0)
            .ok_or_else(|| serde::de::Error::custom("Invalid book decimal"))?;
        let (mantissa, exponent) = token.split_once(['e', 'E']).map_or((token, 0), |(m, e)| {
            (m, e.parse::<i32>().unwrap_or(i32::MAX))
        });
        if mantissa.len() > 128
            || !(-128..=128).contains(&exponent)
            || !mantissa.bytes().all(|b| b.is_ascii_digit() || b == b'.')
        {
            return Err(serde::de::Error::custom("Invalid book decimal"));
        }
        let mut digits = mantissa.replace('.', "");
        let decimal = mantissa.find('.').unwrap_or(mantissa.len()) as i32 + exponent;
        if decimal > digits.len() as i32 {
            digits.extend(std::iter::repeat_n('0', decimal as usize - digits.len()));
        }
        let digits = digits.trim_start_matches('0').to_owned();
        Ok(Self { value, digits })
    }
}
#[derive(Deserialize)]
pub struct Level {
    pub price: Decimal,
    pub qty: Decimal,
}
#[derive(Deserialize)]
pub struct Depth<'a> {
    pub checksum: Option<u32>,
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
/// Kraken Spot OHLC minutes. Only exact intervals: a smaller native interval
/// would publish its candles under the requested one.
pub fn interval(tf: Interval) -> Option<u32> {
    use Interval::*;
    Some(match tf {
        M1 => 1,
        M5 => 5,
        M15 => 15,
        M30 => 30,
        H1 => 60,
        H4 => 240,
        D1 => 1440,
        W1 => 10080,
        _ => return None,
    })
}
pub fn asset(a: &str) -> &str {
    match a {
        "XBT" | "XXBT" => "BTC",
        "XDG" | "XXDG" => "DOGE",
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
