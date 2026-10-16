//! Authoritative V3 futures/BBO/Trade composition; no live runtime consumes it.
use serde::{Deserialize, Serialize};

fn present<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: Deserialize<'de>,
{
    T::deserialize(deserializer).map(Some)
}

use crate::v3_public::{WooV3FuturesRow, WooV3FuturesStats, map_woo_v3_futures_stats};
use crate::v3_ws_quotes::{WooV3WsBbo, WooV3WsTrade};
use safe_cex_types::Market;

// ts: WooV3AuthoritativeTickerInfo
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3AuthoritativeTickerInfo {
    pub futures_stats: WooV3FuturesStats,
    pub bbo: WooV3WsBbo,
    pub trade: WooV3WsTrade,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AuthoritativeTickerInfo")]
struct WooV3AuthoritativeTickerInfoWire {
    #[serde(rename = "futuresStats")]
    futures_stats: WooV3FuturesStats,
    #[serde(rename = "bbo")]
    bbo: WooV3WsBbo,
    #[serde(rename = "trade")]
    trade: WooV3WsTrade,
}
impl Serialize for WooV3AuthoritativeTickerInfo {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3AuthoritativeTickerInfoWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3AuthoritativeTickerInfo {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3AuthoritativeTickerInfoWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AuthoritativeTickerInfo {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { futures_stats, bbo, trade });
    }
}

// ts: WooV3AuthoritativeTicker
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3AuthoritativeTicker {
    pub id: String,
    pub symbol: String,
    pub bid: f64,
    pub ask: f64,
    pub last: f64,
    pub mark: f64,
    pub index: f64,
    pub percentage: f64,
    pub open_interest: f64,
    pub funding_rate: f64,
    pub funding_time: f64,
    pub volume: f64,
    pub quote_volume: f64,
    pub mid_price: Option<f64>,
    pub info: WooV3AuthoritativeTickerInfo,
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3AuthoritativeTicker")]
struct WooV3AuthoritativeTickerWire {
    #[serde(rename = "id")]
    id: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "bid")]
    bid: f64,
    #[serde(rename = "ask")]
    ask: f64,
    #[serde(rename = "last")]
    last: f64,
    #[serde(rename = "mark")]
    mark: f64,
    #[serde(rename = "index")]
    index: f64,
    #[serde(rename = "percentage")]
    percentage: f64,
    #[serde(rename = "openInterest")]
    open_interest: f64,
    #[serde(rename = "fundingRate")]
    funding_rate: f64,
    #[serde(rename = "fundingTime")]
    funding_time: f64,
    #[serde(rename = "volume")]
    volume: f64,
    #[serde(rename = "quoteVolume")]
    quote_volume: f64,
    #[serde(
        rename = "midPrice",
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    mid_price: Option<f64>,
    #[serde(rename = "info")]
    info: WooV3AuthoritativeTickerInfo,
}
impl Serialize for WooV3AuthoritativeTicker {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3AuthoritativeTickerWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3AuthoritativeTicker {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3AuthoritativeTickerWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3AuthoritativeTicker {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, symbol, bid, ask, last, mark, index, percentage, open_interest, funding_rate, funding_time, volume, quote_volume, mid_price, info });
    }
}

// ts: normalizePerpetualSymbol
pub fn normalize_perpetual_symbol(symbol: &str, path: &str) -> Result<String, String> {
    let parts: Vec<_> = symbol.split('_').collect();
    let valid_part = |part: &str| {
        !part.is_empty()
            && part
                .bytes()
                .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit())
    };
    if parts.len() != 3 || parts[0] != "PERP" || !valid_part(parts[1]) || !valid_part(parts[2]) {
        return Err(format!(
            "{path} must be a documented WOO PERP_BASE_QUOTE symbol"
        ));
    }
    Ok(format!("{}{}", parts[1], parts[2]))
}
// ts: requirePositiveFinitePrice
pub fn require_positive_finite_price(value: &str, path: &str) -> Result<f64, String> {
    let price = safe_cex_num::number_from_string(value);
    if !price.is_finite() || price <= 0.0 {
        return Err(format!("{path} must convert to a positive finite price"));
    }
    Ok(price)
}
// ts: composeWooV3AuthoritativeTicker
pub fn compose_woo_v3_authoritative_ticker(
    futures_row: &WooV3FuturesRow,
    bbo: &WooV3WsBbo,
    trade: &WooV3WsTrade,
    market: &Market,
) -> Result<WooV3AuthoritativeTicker, String> {
    let futures_symbol =
        normalize_perpetual_symbol(&futures_row.symbol, "WOO V3 futures ticker symbol")?;
    let bbo_symbol = normalize_perpetual_symbol(&bbo.symbol, "WOO V3 BBO ticker symbol")?;
    let trade_symbol = normalize_perpetual_symbol(&trade.symbol, "WOO V3 Trade ticker symbol")?;
    if futures_symbol != bbo_symbol
        || futures_symbol != trade_symbol
        || futures_symbol != market.symbol
    {
        return Err("WOO V3 ticker sources must have matching normalized symbol identities".into());
    }
    let markets = std::collections::BTreeMap::from([(market.symbol.clone(), market.clone())]);
    let futures_stats = map_woo_v3_futures_stats(std::slice::from_ref(futures_row), &markets)?
        .into_iter()
        .next()
        .ok_or_else(|| "WOO V3 futures ticker row does not map to its market".to_owned())?;
    Ok(WooV3AuthoritativeTicker {
        id: futures_stats
            .id
            .clone()
            .ok_or_else(|| "WOO V3 futures ticker requires a market id".to_owned())?,
        symbol: futures_stats
            .symbol
            .clone()
            .ok_or_else(|| "WOO V3 futures ticker requires a market symbol".to_owned())?,
        bid: require_positive_finite_price(&bbo.bid_price, "WOO V3 ticker bid")?,
        ask: require_positive_finite_price(&bbo.ask_price, "WOO V3 ticker ask")?,
        last: require_positive_finite_price(&trade.price, "WOO V3 ticker last trade")?,
        mark: futures_stats.mark,
        index: futures_stats.index,
        percentage: futures_stats.percentage24h,
        open_interest: futures_stats.open_interest,
        funding_rate: futures_stats.estimated_funding_rate,
        funding_time: futures_stats.funding_time,
        volume: futures_stats.volume24h,
        quote_volume: futures_stats.quote_volume24h,
        mid_price: None,
        info: WooV3AuthoritativeTickerInfo {
            futures_stats,
            bbo: bbo.clone(),
            trade: trade.clone(),
        },
    })
}
