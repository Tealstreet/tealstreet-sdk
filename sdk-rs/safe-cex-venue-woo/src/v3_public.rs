//! Strict, transport-free V3 public decoders; no live route consumes this module.
use crate::v3_contracts::{
    WooV3Instrument, WooV3InstrumentBaseAssetMultiplier, WooV3InstrumentBaseIMR,
    WooV3InstrumentBaseMax, WooV3InstrumentBaseMin, WooV3InstrumentBaseTick,
    WooV3InstrumentMinNotional, WooV3InstrumentQuoteTick,
};
use serde::{Deserialize, Serialize};
use serde_json::{Map, Value};

// ts: WooV3FuturesRow
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3FuturesRow {
    pub symbol: String,
    pub index_price: String,
    pub mark_price: String,
    pub est_funding_rate: String,
    pub last_funding_rate: String,
    pub open_interest: String,
    pub open_24h: String,
    pub close_24h: String,
    pub high_24h: String,
    pub low_24h: String,
    pub volume_24h: String,
    pub amount_24h: String,
    pub next_funding_time: f64,
}

// ts: WooV3OrderBookLevel
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3OrderBookLevel {
    pub price: String,
    pub quantity: String,
}

// ts: WooV3OrderBookSnapshot
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3OrderBookSnapshot {
    pub timestamp: f64,
    pub asks: Vec<WooV3OrderBookLevel>,
    pub bids: Vec<WooV3OrderBookLevel>,
}

// ts: WooV3KlineRow
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3KlineRow {
    pub open: String,
    pub close: String,
    pub low: String,
    pub high: String,
    pub volume: String,
    pub amount: String,
    pub symbol: String,
    pub r#type: String,
    pub start_timestamp: f64,
    pub end_timestamp: f64,
}

// ts: WooV3FuturesStats
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3FuturesStats {
    pub id: Option<String>,
    pub symbol: Option<String>,
    pub index: f64,
    pub mark: f64,
    pub open24h: f64,
    pub close24h: f64,
    pub high24h: f64,
    pub low24h: f64,
    pub percentage24h: f64,
    pub open_interest: f64,
    pub estimated_funding_rate: f64,
    pub last_funding_rate: f64,
    pub funding_time: f64,
    pub volume24h: f64,
    pub quote_volume24h: f64,
}

// ts: WooV3MappedOrderBookSnapshot
#[derive(Debug, Clone, PartialEq)]
pub struct WooV3MappedOrderBookSnapshot {
    pub timestamp: f64,
    pub book: safe_cex_types::OrderBook,
}

// ts: isRecord
pub fn is_record(value: Option<&Value>) -> bool {
    value.is_some_and(Value::is_object)
}
// ts: requireRecord
pub fn require_record<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<&'a Map<String, Value>, String> {
    value
        .and_then(Value::as_object)
        .ok_or_else(|| format!("{path} must be an object"))
}
// ts: requireArray
pub fn require_array<'a>(value: Option<&'a Value>, path: &str) -> Result<&'a [Value], String> {
    value
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| format!("{path} must be an array"))
}
// ts: requireString
pub fn require_string(value: Option<&Value>, path: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .ok_or_else(|| format!("{path} must be a non-empty string"))
}
// ts: optionalString
pub fn optional_string(value: Option<&Value>, path: &str) -> Result<Option<String>, String> {
    value.map(|v| require_string(Some(v), path)).transpose()
}
// ts: requireInteger
pub fn require_integer(value: Option<&Value>, path: &str) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && (0.0..=9_007_199_254_740_991.0).contains(n))
        .ok_or_else(|| format!("{path} must be a non-negative safe integer"))
}
// ts: requireTimestamp
pub fn require_timestamp(value: Option<&Value>, path: &str) -> Result<f64, String> {
    value
        .and_then(Value::as_f64)
        .filter(|n| n.fract() == 0.0 && *n > 0.0 && *n <= 9_007_199_254_740_991.0)
        .ok_or_else(|| format!("{path} must be a positive safe integer millisecond timestamp"))
}
// ts: requireBoolean
pub fn require_boolean(value: Option<&Value>, path: &str) -> Result<bool, String> {
    value
        .and_then(Value::as_bool)
        .ok_or_else(|| format!("{path} must be a boolean"))
}
// ts: requireDecimalString
pub fn require_decimal_string(value: Option<&Value>, path: &str) -> Result<String, String> {
    value
        .and_then(Value::as_str)
        .filter(|s| {
            !safe_cex_num::js_trim(s).is_empty() && safe_cex_num::number_from_string(s).is_finite()
        })
        .map(str::to_owned)
        .ok_or_else(|| format!("{path} must be a finite decimal string"))
}
// ts: requireInstrumentDecimal
pub fn require_instrument_decimal<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<&'a Value, String> {
    let value = value.ok_or_else(|| format!("{path} must be a finite decimal string"))?;
    if value.as_f64().is_some_and(f64::is_finite) {
        return Ok(value);
    }
    require_decimal_string(Some(value), path)?;
    Ok(value)
}
// ts: optionalInstrumentDecimal
pub fn optional_instrument_decimal<'a>(
    value: Option<&'a Value>,
    path: &str,
) -> Result<Option<&'a Value>, String> {
    value
        .map(|v| require_instrument_decimal(Some(v), path))
        .transpose()
}
// ts: requireFiniteNumber
pub fn require_finite_number(
    value: Option<&Value>,
    path: &str,
    minimum: f64,
) -> Result<f64, String> {
    let result = match value {
        None => f64::NAN,
        Some(Value::Null) => 0.0,
        Some(Value::Bool(b)) => {
            if *b {
                1.0
            } else {
                0.0
            }
        }
        Some(Value::Number(n)) => n.as_f64().unwrap_or(f64::NAN),
        other => safe_cex_num::number_from_string(
            &safe_cex_num::js_string(other).map_err(|e| e.message.to_owned())?,
        ),
    };
    if !result.is_finite() || result < minimum {
        return Err(format!(
            "{path} must be at least {}",
            safe_cex_num::js_number_string(minimum)
        ));
    }
    Ok(result)
}
// ts: decodeEnvelopeWithTimestamp
pub fn decode_envelope_with_timestamp<'a>(
    input: &'a Value,
    name: &str,
    timestamp_decoder: fn(Option<&Value>, &str) -> Result<f64, String>,
) -> Result<(&'a Map<String, Value>, f64), String> {
    let response = require_record(Some(input), &format!("WOO V3 {name} response"))?;
    if !require_boolean(response.get("success"), &format!("WOO V3 {name}.success"))? {
        return Err(format!("WOO V3 {name} response reports success=false"));
    }
    let data = require_record(response.get("data"), &format!("WOO V3 {name}.data"))?;
    let timestamp = timestamp_decoder(
        response.get("timestamp"),
        &format!("WOO V3 {name}.timestamp"),
    )?;
    Ok((data, timestamp))
}
// ts: decodeEnvelope
pub fn decode_envelope<'a>(input: &'a Value, name: &str) -> Result<&'a Map<String, Value>, String> {
    Ok(decode_envelope_with_timestamp(input, name, require_integer)?.0)
}
// ts: decodeRows
pub fn decode_rows<T>(
    input: &Value,
    name: &str,
    decoder: impl Fn(&Value, &str) -> Result<T, String>,
) -> Result<Vec<T>, String> {
    let data = decode_envelope(input, name)?;
    require_array(data.get("rows"), &format!("WOO V3 {name}.data.rows"))?
        .iter()
        .enumerate()
        .map(|(i, row)| decoder(row, &format!("WOO V3 {name}.data.rows[{i}]")))
        .collect()
}

macro_rules! decimal_field {
    ($row:ident,$path:ident,$key:literal,$ty:ident) => {{
        let v = require_instrument_decimal($row.get($key), &format!("{}.{}", $path, $key))?;
        match v {
            Value::String(s) => $ty::Decimal(s.clone()),
            _ => $ty::Number(require_finite_number(
                Some(v),
                &format!("{}.{}", $path, $key),
                f64::NEG_INFINITY,
            )?),
        }
    }};
}
// ts: decodeInstrument
pub fn decode_instrument(input: &Value, path: &str) -> Result<WooV3Instrument, String> {
    let row = require_record(Some(input), path)?;
    Ok(WooV3Instrument {
        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"))?,
        status: optional_string(row.get("status"), &format!("{path}.status"))?,
        base_asset: require_string(row.get("baseAsset"), &format!("{path}.baseAsset"))?,
        quote_asset: require_string(row.get("quoteAsset"), &format!("{path}.quoteAsset"))?,
        base_asset_multiplier: optional_instrument_decimal(
            row.get("baseAssetMultiplier"),
            &format!("{path}.baseAssetMultiplier"),
        )?
        .map(|v| match v {
            Value::String(s) => Ok(WooV3InstrumentBaseAssetMultiplier::Decimal(s.clone())),
            _ => require_finite_number(
                Some(v),
                &format!("{path}.baseAssetMultiplier"),
                f64::NEG_INFINITY,
            )
            .map(WooV3InstrumentBaseAssetMultiplier::Number),
        })
        .transpose()?,
        base_min: decimal_field!(row, path, "baseMin", WooV3InstrumentBaseMin),
        base_max: decimal_field!(row, path, "baseMax", WooV3InstrumentBaseMax),
        base_tick: decimal_field!(row, path, "baseTick", WooV3InstrumentBaseTick),
        quote_tick: decimal_field!(row, path, "quoteTick", WooV3InstrumentQuoteTick),
        min_notional: decimal_field!(row, path, "minNotional", WooV3InstrumentMinNotional),
        base_imr: decimal_field!(row, path, "baseIMR", WooV3InstrumentBaseIMR),
        funding_interval_hours: row
            .get("fundingIntervalHours")
            .map(|v| require_integer(Some(v), &format!("{path}.fundingIntervalHours")))
            .transpose()?,
        is_allowed_rpi: row
            .get("isAllowedRpi")
            .map(|v| require_boolean(Some(v), &format!("{path}.isAllowedRpi")))
            .transpose()?,
    })
}

// ts: decodeFuturesRow
pub fn decode_futures_row(input: &Value, path: &str) -> Result<WooV3FuturesRow, String> {
    let row = require_record(Some(input), path)?;
    Ok(WooV3FuturesRow {
        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"))?,
        index_price: require_decimal_string(row.get("indexPrice"), &format!("{path}.indexPrice"))?,
        mark_price: require_decimal_string(row.get("markPrice"), &format!("{path}.markPrice"))?,
        est_funding_rate: require_decimal_string(
            row.get("estFundingRate"),
            &format!("{path}.estFundingRate"),
        )?,
        last_funding_rate: require_decimal_string(
            row.get("lastFundingRate"),
            &format!("{path}.lastFundingRate"),
        )?,
        open_interest: require_decimal_string(
            row.get("openInterest"),
            &format!("{path}.openInterest"),
        )?,
        open_24h: require_decimal_string(row.get("24hOpen"), &format!("{path}.24hOpen"))?,
        close_24h: require_decimal_string(row.get("24hClose"), &format!("{path}.24hClose"))?,
        high_24h: require_decimal_string(row.get("24hHigh"), &format!("{path}.24hHigh"))?,
        low_24h: require_decimal_string(row.get("24hLow"), &format!("{path}.24hLow"))?,
        volume_24h: require_decimal_string(row.get("24hVolume"), &format!("{path}.24hVolume"))?,
        amount_24h: require_decimal_string(row.get("24hAmount"), &format!("{path}.24hAmount"))?,
        next_funding_time: require_integer(
            row.get("nextFundingTime"),
            &format!("{path}.nextFundingTime"),
        )?,
    })
}

// ts: decodeOrderBookLevel
pub fn decode_order_book_level(input: &Value, path: &str) -> Result<WooV3OrderBookLevel, String> {
    let row = require_record(Some(input), path)?;
    Ok(WooV3OrderBookLevel {
        price: require_decimal_string(row.get("price"), &format!("{path}.price"))?,
        quantity: require_decimal_string(row.get("quantity"), &format!("{path}.quantity"))?,
    })
}

// ts: decodeKlineRow
pub fn decode_kline_row(input: &Value, path: &str) -> Result<WooV3KlineRow, String> {
    let row = require_record(Some(input), path)?;
    let start_timestamp =
        require_integer(row.get("startTimestamp"), &format!("{path}.startTimestamp"))?;
    let end_timestamp = require_integer(row.get("endTimestamp"), &format!("{path}.endTimestamp"))?;
    if end_timestamp < start_timestamp {
        return Err(format!(
            "{path}.endTimestamp must not precede startTimestamp"
        ));
    }
    Ok(WooV3KlineRow {
        open: require_decimal_string(row.get("open"), &format!("{path}.open"))?,
        close: require_decimal_string(row.get("close"), &format!("{path}.close"))?,
        low: require_decimal_string(row.get("low"), &format!("{path}.low"))?,
        high: require_decimal_string(row.get("high"), &format!("{path}.high"))?,
        volume: require_decimal_string(row.get("volume"), &format!("{path}.volume"))?,
        amount: require_decimal_string(row.get("amount"), &format!("{path}.amount"))?,

        symbol: require_string(row.get("symbol"), &format!("{path}.symbol"))?,
        r#type: require_string(row.get("type"), &format!("{path}.type"))?,
        start_timestamp,
        end_timestamp,
    })
}
// ts: decodeWooV3Instruments
pub fn decode_woo_v3_instruments(input: &Value) -> Result<Vec<WooV3Instrument>, String> {
    decode_rows(input, "instruments", decode_instrument)
}
// ts: decodeWooV3Futures
pub fn decode_woo_v3_futures(input: &Value) -> Result<Vec<WooV3FuturesRow>, String> {
    decode_rows(input, "futures", decode_futures_row)
}
// ts: decodeWooV3Klines
pub fn decode_woo_v3_klines(input: &Value) -> Result<Vec<WooV3KlineRow>, String> {
    decode_rows(input, "kline", decode_kline_row)
}
// ts: decodeWooV3OrderBook
pub fn decode_woo_v3_order_book(input: &Value) -> Result<WooV3OrderBookSnapshot, String> {
    let (data, timestamp) = decode_envelope_with_timestamp(input, "orderbook", require_timestamp)?;
    let asks = require_array(data.get("asks"), "WOO V3 orderbook.data.asks")?
        .iter()
        .enumerate()
        .map(|(i, v)| decode_order_book_level(v, &format!("WOO V3 orderbook.data.asks[{i}]")))
        .collect::<Result<_, _>>()?;
    let bids = require_array(data.get("bids"), "WOO V3 orderbook.data.bids")?
        .iter()
        .enumerate()
        .map(|(i, v)| decode_order_book_level(v, &format!("WOO V3 orderbook.data.bids[{i}]")))
        .collect::<Result<_, _>>()?;
    Ok(WooV3OrderBookSnapshot {
        timestamp,
        asks,
        bids,
    })
}
// ts: mapWooV3Markets
pub fn map_woo_v3_markets(rows: &[WooV3Instrument]) -> Result<Vec<safe_cex_types::Market>, String> {
    use safe_cex_types::{
        Market, MarketLimits, MarketLimitsAmount, MarketLimitsLeverage, MarketPrecision,
    };
    rows.iter()
        .filter(|row| row.symbol.starts_with("PERP_"))
        .enumerate()
        .map(|(index, row)| {
            let path = format!("WOO V3 instrument[{index}]");
            let amount_min = {
                let value = match &row.base_min {
                    WooV3InstrumentBaseMin::Number(value) => *value,
                    WooV3InstrumentBaseMin::Decimal(value) => {
                        safe_cex_num::number_from_string(value)
                    }
                };
                if !value.is_finite() || value < f64::from_bits(1) {
                    return Err(format!(
                        "{path}.baseMin must be at least {}",
                        safe_cex_num::js_number_string(f64::from_bits(1))
                    ));
                }
                value
            };
            let amount_max = {
                let value = match &row.base_max {
                    WooV3InstrumentBaseMax::Number(value) => *value,
                    WooV3InstrumentBaseMax::Decimal(value) => {
                        safe_cex_num::number_from_string(value)
                    }
                };
                if !value.is_finite() || value < amount_min {
                    return Err(format!(
                        "{path}.baseMax must be at least {}",
                        safe_cex_num::js_number_string(amount_min)
                    ));
                }
                value
            };
            let base_imr = {
                let value = match &row.base_imr {
                    WooV3InstrumentBaseIMR::Number(value) => *value,
                    WooV3InstrumentBaseIMR::Decimal(value) => {
                        safe_cex_num::number_from_string(value)
                    }
                };
                if !value.is_finite() || value < f64::from_bits(1) {
                    return Err(format!(
                        "{path}.baseIMR must be at least {}",
                        safe_cex_num::js_number_string(f64::from_bits(1))
                    ));
                }
                value
            };
            if base_imr > 1.0 {
                return Err(format!("{path}.baseIMR must not exceed 1"));
            }
            Ok(Market {
                runtime: Default::default(),
                id: format!("{}/{}:{}", row.base_asset, row.quote_asset, row.quote_asset),
                symbol: format!("{}{}", row.base_asset, row.quote_asset),
                base: row.base_asset.clone(),
                quote: row.quote_asset.clone(),
                active: row
                    .status
                    .as_deref()
                    .is_none_or(|status| status == "TRADING"),
                precision: MarketPrecision {
                    base: None,
                    quote: None,
                    amount: {
                        let value = match &row.base_tick {
                            WooV3InstrumentBaseTick::Number(value) => *value,
                            WooV3InstrumentBaseTick::Decimal(value) => {
                                safe_cex_num::number_from_string(value)
                            }
                        };
                        if !value.is_finite() || value < f64::from_bits(1) {
                            return Err(format!(
                                "{path}.baseTick must be at least {}",
                                safe_cex_num::js_number_string(f64::from_bits(1))
                            ));
                        }
                        value
                    },
                    price: {
                        let value = match &row.quote_tick {
                            WooV3InstrumentQuoteTick::Number(value) => *value,
                            WooV3InstrumentQuoteTick::Decimal(value) => {
                                safe_cex_num::number_from_string(value)
                            }
                        };
                        if !value.is_finite() || value < f64::from_bits(1) {
                            return Err(format!(
                                "{path}.quoteTick must be at least {}",
                                safe_cex_num::js_number_string(f64::from_bits(1))
                            ));
                        }
                        value
                    },
                },
                limits: MarketLimits {
                    cost: None,
                    amount: MarketLimitsAmount {
                        min: amount_min,
                        max: (amount_max).into(),
                    },
                    leverage: MarketLimitsLeverage {
                        min: 1.0,
                        max: (1.0 / base_imr).floor(),
                    },
                    price: None,
                },
                info: Some(serde_json::to_value(row).map_err(|e| e.to_string())?.into()),
                pretty_symbol: None,
                contract_size: None,
                is_expiring: None,
                is_inverse: None,
                is_spot: None,
                is_perpetual: None,
                is_read_only: None,
                taker_fee: None,
                maker_fee: None,
                option: None,
            })
        })
        .collect()
}
// ts: mapWooV3OrderBook
pub fn map_woo_v3_order_book(
    snapshot: &WooV3OrderBookSnapshot,
) -> Result<WooV3MappedOrderBookSnapshot, String> {
    let map_side = |levels: &[WooV3OrderBookLevel], side: &str| {
        levels
            .iter()
            .enumerate()
            .map(|(i, v)| {
                Ok(safe_cex_types::OrderBookOrders {
                    price: require_finite_number(
                        Some(&Value::String(v.price.clone())),
                        &format!("WOO V3 orderbook.{side}[{i}].price"),
                        f64::from_bits(1),
                    )?,
                    amount: require_finite_number(
                        Some(&Value::String(v.quantity.clone())),
                        &format!("WOO V3 orderbook.{side}[{i}].quantity"),
                        f64::from_bits(1),
                    )?,
                    total: 0.0,
                })
            })
            .collect::<Result<Vec<_>, String>>()
    };
    let timestamp = require_timestamp(
        Some(&serde_json::json!(snapshot.timestamp)),
        "WOO V3 orderbook.timestamp",
    )?;
    let mut book = safe_cex_types::OrderBook {
        bids: map_side(&snapshot.bids, "bids")?,
        asks: map_side(&snapshot.asks, "asks")?,
    };
    safe_cex_store::orderbook::sort_order_book(&mut book);
    safe_cex_store::orderbook::calc_order_book_total(&mut book);
    Ok(WooV3MappedOrderBookSnapshot { timestamp, book })
}
// Map-only record boundaries. Remote companions are private.

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3FuturesRow")]
struct WooV3FuturesRowWire {
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "indexPrice")]
    index_price: String,
    #[serde(rename = "markPrice")]
    mark_price: String,
    #[serde(rename = "estFundingRate")]
    est_funding_rate: String,
    #[serde(rename = "lastFundingRate")]
    last_funding_rate: String,
    #[serde(rename = "openInterest")]
    open_interest: String,
    #[serde(rename = "24hOpen")]
    open_24h: String,
    #[serde(rename = "24hClose")]
    close_24h: String,
    #[serde(rename = "24hHigh")]
    high_24h: String,
    #[serde(rename = "24hLow")]
    low_24h: String,
    #[serde(rename = "24hVolume")]
    volume_24h: String,
    #[serde(rename = "24hAmount")]
    amount_24h: String,
    #[serde(rename = "nextFundingTime")]
    next_funding_time: f64,
}
impl Serialize for WooV3FuturesRow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3FuturesRowWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3FuturesRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3FuturesRowWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3OrderBookLevel")]
struct WooV3OrderBookLevelWire {
    #[serde(rename = "price")]
    price: String,
    #[serde(rename = "quantity")]
    quantity: String,
}
impl Serialize for WooV3OrderBookLevel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3OrderBookLevelWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3OrderBookLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3OrderBookLevelWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3OrderBookSnapshot")]
struct WooV3OrderBookSnapshotWire {
    #[serde(rename = "timestamp")]
    timestamp: f64,
    #[serde(rename = "asks")]
    asks: Vec<WooV3OrderBookLevel>,
    #[serde(rename = "bids")]
    bids: Vec<WooV3OrderBookLevel>,
}
impl Serialize for WooV3OrderBookSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3OrderBookSnapshotWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3OrderBookSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3OrderBookSnapshotWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3KlineRow")]
struct WooV3KlineRowWire {
    #[serde(rename = "open")]
    open: String,
    #[serde(rename = "close")]
    close: String,
    #[serde(rename = "low")]
    low: String,
    #[serde(rename = "high")]
    high: String,
    #[serde(rename = "volume")]
    volume: String,
    #[serde(rename = "amount")]
    amount: String,
    #[serde(rename = "symbol")]
    symbol: String,
    #[serde(rename = "type")]
    r#type: String,
    #[serde(rename = "startTimestamp")]
    start_timestamp: f64,
    #[serde(rename = "endTimestamp")]
    end_timestamp: f64,
}
impl Serialize for WooV3KlineRow {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3KlineRowWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3KlineRow {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3KlineRowWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3FuturesStats")]
struct WooV3FuturesStatsWire {
    #[serde(
        rename = "id",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "required_stats_string"
    )]
    id: Option<String>,
    #[serde(
        rename = "symbol",
        skip_serializing_if = "Option::is_none",
        deserialize_with = "required_stats_string"
    )]
    symbol: Option<String>,
    #[serde(rename = "index")]
    index: f64,
    #[serde(rename = "mark")]
    mark: f64,
    #[serde(rename = "open24h")]
    open24h: f64,
    #[serde(rename = "close24h")]
    close24h: f64,
    #[serde(rename = "high24h")]
    high24h: f64,
    #[serde(rename = "low24h")]
    low24h: f64,
    #[serde(rename = "percentage24h")]
    percentage24h: f64,
    #[serde(rename = "openInterest")]
    open_interest: f64,
    #[serde(rename = "estimatedFundingRate")]
    estimated_funding_rate: f64,
    #[serde(rename = "lastFundingRate")]
    last_funding_rate: f64,
    #[serde(rename = "fundingTime")]
    funding_time: f64,
    #[serde(rename = "volume24h")]
    volume24h: f64,
    #[serde(rename = "quoteVolume24h")]
    quote_volume24h: f64,
}
impl Serialize for WooV3FuturesStats {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3FuturesStatsWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3FuturesStats {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3FuturesStatsWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

#[derive(Serialize, Deserialize)]
#[serde(remote = "WooV3MappedOrderBookSnapshot")]
struct WooV3MappedOrderBookSnapshotWire {
    timestamp: f64,
    #[serde(flatten)]
    book: safe_cex_types::OrderBook,
}
impl Serialize for WooV3MappedOrderBookSnapshot {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        WooV3MappedOrderBookSnapshotWire::serialize(self, serializer)
    }
}
impl<'de> Deserialize<'de> for WooV3MappedOrderBookSnapshot {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        safe_cex_num::numeric_ingress::JsNumberInput::deserialize(deserializer)?
            .decode_with(|decoder| {
                WooV3MappedOrderBookSnapshotWire::deserialize(
                    safe_cex_types::strict_decode::object(decoder),
                )
            })
            .map_err(serde::de::Error::custom)
    }
}

// The declared input requires strings; the buggy mapper can emit undefined.
fn required_stats_string<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> Result<Option<String>, D::Error> {
    String::deserialize(d).map(Some)
}

// ts: mapWooV3FuturesStats
pub fn map_woo_v3_futures_stats(
    rows: &[WooV3FuturesRow],
    markets: &std::collections::BTreeMap<String, safe_cex_types::Market>,
) -> Result<Vec<WooV3FuturesStats>, String> {
    use safe_cex_num::{TableHit, js_table_lookup};
    let entries: Vec<_> = markets
        .iter()
        .map(|(key, value)| (key.as_str(), value))
        .collect();
    let mut result = Vec::new();
    for (index, row) in rows.iter().enumerate() {
        let normalized = row
            .symbol
            .strip_prefix("PERP_")
            .unwrap_or(&row.symbol)
            .replace('_', "");
        let mut hit = js_table_lookup(&entries, &normalized);
        // Source uses ??, not membership or truthiness. Typed Market values are non-null.
        if hit.is_nullish_by(|_| false) {
            hit = js_table_lookup(&entries, &row.symbol);
        }
        let market = match hit {
            TableHit::Mapped(market) => Some(*market),
            TableHit::Inherited(_) => None,
            TableHit::Missing => continue,
        };
        let path = format!("WOO V3 futures[{index}]");
        let number = |value: &str, key: &str, minimum| {
            require_finite_number(
                Some(&Value::String(value.into())),
                &format!("{path}.{key}"),
                minimum,
            )
        };
        let open24h = number(&row.open_24h, "24hOpen", f64::from_bits(1))?;
        let close24h = number(&row.close_24h, "24hClose", 0.0)?;
        // ts-bug: inherited Object.prototype entries have undefined id/symbol.
        result.push(WooV3FuturesStats {
            id: market.map(|v| v.id.clone()),
            symbol: market.map(|v| v.symbol.clone()),
            index: number(&row.index_price, "indexPrice", 0.0)?,
            mark: number(&row.mark_price, "markPrice", 0.0)?,
            open24h,
            close24h,
            high24h: number(&row.high_24h, "24hHigh", 0.0)?,
            low24h: number(&row.low_24h, "24hLow", 0.0)?,
            percentage24h: safe_cex_num::math_round(((close24h - open24h) / open24h) * 10_000.0)
                / 100.0,
            open_interest: number(&row.open_interest, "openInterest", 0.0)?,
            estimated_funding_rate: number(
                &row.est_funding_rate,
                "estFundingRate",
                f64::NEG_INFINITY,
            )?,
            last_funding_rate: number(
                &row.last_funding_rate,
                "lastFundingRate",
                f64::NEG_INFINITY,
            )?,
            funding_time: row.next_funding_time,
            volume24h: number(&row.volume_24h, "24hVolume", 0.0)?,
            quote_volume24h: number(&row.amount_24h, "24hAmount", 0.0)?,
        });
    }
    Ok(result)
}

// Explicit registrations keep the complete output graph visible to parity indexing.
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3FuturesRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { symbol, index_price, mark_price, est_funding_rate, last_funding_rate, open_interest, open_24h, close_24h, high_24h, low_24h, volume_24h, amount_24h, next_funding_time });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3OrderBookLevel {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { price, quantity });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3OrderBookSnapshot {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { timestamp, asks, bids });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3KlineRow {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { open, close, low, high, volume, amount, symbol, r#type, start_timestamp, end_timestamp });
    }
}
impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3FuturesStats {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { id, symbol, index, mark, open24h, close24h, high24h, low24h, percentage24h, open_interest, estimated_funding_rate, last_funding_rate, funding_time, volume24h, quote_volume24h });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for WooV3MappedOrderBookSnapshot {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { timestamp, book });
    }
}
