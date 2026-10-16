//! Inert Starbase public-data contracts and validation.
use crate::starbase::CoinbaseStarbaseProtocolError;
use serde::Serialize;
use serde_json::Value;

// ts: coinbase.starbase-public-frames.ts::isSupportedPublicChannel
pub fn is_supported_public_channel(channel: &str) -> bool {
    const INSTRUMENTS: [&str; 2] = ["BTC_USDC-PERPETUAL", "ETH_USDC-PERPETUAL"];
    const BOOK_INTERVALS: [&str; 2] = ["100ms", "agg2"];
    const CHART_RESOLUTIONS: [&str; 10] =
        ["1", "3", "5", "15", "30", "60", "120", "360", "720", "1D"];

    let parts = channel.split('.').collect::<Vec<_>>();
    match parts.as_slice() {
        [category, instrument, interval] if *category == "ticker" || *category == "book" => {
            INSTRUMENTS.contains(instrument) && BOOK_INTERVALS.contains(interval)
        }
        ["chart", "trades", instrument, resolution] => {
            INSTRUMENTS.contains(instrument) && CHART_RESOLUTIONS.contains(resolution)
        }
        ["instrument", "state", "future", currency] => {
            ["BTC", "ETH", "USDC", "any"].contains(currency)
        }
        _ => false,
    }
}

// ts: coinbase.starbase-public-frames.ts::hasExactKeys
pub fn has_exact_keys(value: &serde_json::Map<String, Value>, expected: &[&str]) -> bool {
    let keys = safe_cex_num::js_own_keys(value.keys().map(String::as_str));
    keys.len() == expected.len() && keys.iter().all(|key| expected.contains(key))
}

// ts: parsePublicTestError
pub fn parse_public_test_error(value: Option<&Value>) -> Option<serde_json::Map<String, Value>> {
    let value = value?.as_object()?;
    if value.len() < 2
        || value.len() > 3
        || !value
            .keys()
            .all(|key| ["code", "data", "message"].contains(&key.as_str()))
    {
        return None;
    }
    let code = value.get("code")?.as_f64()?;
    if !code.is_finite() || code.fract() != 0.0 || code.abs() > 9_007_199_254_740_991.0 {
        return None;
    }
    if value.get("message")?.as_str()?.is_empty() {
        return None;
    }
    let mut result = serde_json::Map::new();
    result.insert("code".into(), value.get("code")?.clone());
    result.insert("message".into(), value.get("message")?.clone());
    if let Some(data) = value.get("data") {
        result.insert("data".into(), data.clone());
    }
    Some(result)
}

// ts: validateContext
pub fn validate_context(value: &Value) -> Result<(), CoinbaseStarbaseProtocolError> {
    let validate_channels = |key: &str, label: &str| {
        let channels = value.get(key).and_then(Value::as_array).ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!("Invalid {label} Starbase public channels"))
        })?;
        let mut seen = std::collections::HashSet::with_capacity(channels.len());
        for channel in channels {
            let Some(channel) = channel.as_str() else {
                return Err(CoinbaseStarbaseProtocolError::new(format!(
                    "Invalid {label} Starbase public channels"
                )));
            };
            if !is_supported_public_channel(channel) || !seen.insert(channel) {
                return Err(CoinbaseStarbaseProtocolError::new(format!(
                    "Invalid {label} Starbase public channels"
                )));
            }
        }
        Ok(())
    };

    validate_channels("acknowledgedPublicChannels", "acknowledged")?;
    validate_channels("desiredPublicChannels", "desired")?;
    let request_ids = value
        .get("pendingPublicTestRequestIds")
        .and_then(Value::as_array)
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new("Invalid pending Starbase public/test request IDs")
        })?;
    let mut seen = std::collections::HashSet::with_capacity(request_ids.len());
    for id in request_ids {
        let Some(id) = id.as_f64().filter(|id| {
            id.is_finite() && id.fract() == 0.0 && *id > 0.0 && *id <= 9_007_199_254_740_991.0
        }) else {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Invalid pending Starbase public/test request IDs",
            ));
        };
        if !seen.insert(id as u64) {
            return Err(CoinbaseStarbaseProtocolError::new(
                "Invalid pending Starbase public/test request IDs",
            ));
        }
    }
    Ok(())
}

// ts: CoinbaseStarbaseBaseCurrency
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBaseCurrency {
    #[serde(rename = "BTC")]
    Btc,
    #[serde(rename = "ETH")]
    Eth,
}

// ts: CoinbaseStarbaseBookState
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBookState {
    #[serde(rename = "open")]
    Open,
    #[serde(rename = "settlement")]
    Settlement,
    #[serde(rename = "delivered")]
    Delivered,
    #[serde(rename = "inactive")]
    Inactive,
    #[serde(rename = "locked")]
    Locked,
    #[serde(rename = "halted")]
    Halted,
    #[serde(rename = "archivized")]
    Archivized,
}

// ts: CoinbaseStarbaseBookInterval
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBookInterval {
    #[serde(rename = "100ms")]
    V100Ms,
    #[serde(rename = "agg2")]
    Agg2,
}

// ts: CoinbaseStarbaseChartResolution
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseChartResolution {
    #[serde(rename = "1")]
    V1,
    #[serde(rename = "3")]
    V3,
    #[serde(rename = "5")]
    V5,
    #[serde(rename = "15")]
    V15,
    #[serde(rename = "30")]
    V30,
    #[serde(rename = "60")]
    V60,
    #[serde(rename = "120")]
    V120,
    #[serde(rename = "360")]
    V360,
    #[serde(rename = "720")]
    V720,
    #[serde(rename = "1D")]
    V1D,
}

// Anonymous currency union inside CoinbaseStarbasePublicChannel.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseInstrumentStateCurrency {
    #[serde(rename = "BTC")]
    Btc,
    #[serde(rename = "ETH")]
    Eth,
    #[serde(rename = "USDC")]
    Usdc,
    #[serde(rename = "any")]
    Any,
}

// ts: CoinbaseStarbasePublicChannel
// Serialize-only: no implicit string/object decoder bypasses these closed branches.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CoinbaseStarbasePublicChannel {
    Ticker {
        instrument_name: String,
        interval: CoinbaseStarbaseBookInterval,
    },
    Book {
        instrument_name: String,
        interval: CoinbaseStarbaseBookInterval,
    },
    Chart {
        instrument_name: String,
        resolution: CoinbaseStarbaseChartResolution,
    },
    InstrumentState {
        currency: CoinbaseStarbaseInstrumentStateCurrency,
    },
}

impl Serialize for CoinbaseStarbasePublicChannel {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Consume each enum's canonical serializer rather than duplicating its names.
        fn spelling<T: Serialize, E: serde::ser::Error>(value: &T) -> Result<String, E> {
            match serde_json::to_value(value).map_err(E::custom)? {
                Value::String(value) => Ok(value),
                _ => Err(E::custom(
                    "Starbase channel suffix must serialize as a string",
                )),
            }
        }
        match self {
            Self::Ticker {
                instrument_name,
                interval,
            } => serializer.collect_str(&format_args!(
                "ticker.{instrument_name}.{}",
                spelling::<_, S::Error>(interval)?
            )),
            Self::Book {
                instrument_name,
                interval,
            } => serializer.collect_str(&format_args!(
                "book.{instrument_name}.{}",
                spelling::<_, S::Error>(interval)?
            )),
            Self::Chart {
                instrument_name,
                resolution,
            } => serializer.collect_str(&format_args!(
                "chart.trades.{instrument_name}.{}",
                spelling::<_, S::Error>(resolution)?
            )),
            Self::InstrumentState { currency } => serializer.collect_str(&format_args!(
                "instrument.state.future.{}",
                spelling::<_, S::Error>(currency)?
            )),
        }
    }
}

// ts: CoinbaseStarbaseMarketParityBlocker
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseMarketParityBlocker {
    #[serde(rename = "amount-increment-unpublished")]
    AmountIncrementUnpublished,
    #[serde(rename = "maximum-base-amount-unpublished")]
    MaximumBaseAmountUnpublished,
}

// ts: isRecord
pub fn is_record(value: Option<&Value>) -> bool {
    crate::starbase::is_record(value)
}

// ts: protocolError
pub fn protocol_error<T>(message: impl Into<String>) -> Result<T, CoinbaseStarbaseProtocolError> {
    Err(CoinbaseStarbaseProtocolError::new(message))
}

// ts: requireRecord
pub fn require_record<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a serde_json::Map<String, Value>, CoinbaseStarbaseProtocolError> {
    value.and_then(Value::as_object).ok_or_else(|| {
        CoinbaseStarbaseProtocolError::new(format!("Invalid {context}: expected an object"))
    })
}

// ts: requireString
pub fn require_string<'a>(
    value: Option<&'a Value>,
    context: &str,
) -> Result<&'a str, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_str)
        .filter(|s| !s.is_empty())
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected a non-empty string"
            ))
        })
}

// ts: requireFinite
pub fn require_finite(
    value: Option<&Value>,
    context: &str,
    minimum: Option<f64>,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    let minimum = minimum.unwrap_or(0.0);
    value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && v.partial_cmp(&minimum) != Some(std::cmp::Ordering::Less))
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected a finite number >= {}",
                safe_cex_num::js_number_string(minimum)
            ))
        })
}

// ts: requirePositive
pub fn require_positive(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    let parsed = require_finite(value, context, None)?;
    if parsed <= 0.0 {
        return Err(CoinbaseStarbaseProtocolError::new(format!(
            "Invalid {context}: expected a positive number"
        )));
    }
    Ok(parsed)
}

// ts: requirePositiveInteger
pub fn require_positive_integer(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_f64)
        .filter(|_| crate::starbase::is_safe_request_id(value))
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected a positive safe integer"
            ))
        })
}

// ts: requireTimestamp
pub fn require_timestamp(
    value: Option<&Value>,
    context: &str,
) -> Result<f64, CoinbaseStarbaseProtocolError> {
    value
        .and_then(Value::as_f64)
        .filter(|v| v.is_finite() && v.fract() == 0.0 && *v >= 0.0 && *v <= 9_007_199_254_740_991.0)
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Invalid {context}: expected a non-negative safe integer timestamp"
            ))
        })
}

// ts: parseBookState
pub fn parse_book_state(
    value: Option<&Value>,
    context: &str,
) -> Result<CoinbaseStarbaseBookState, CoinbaseStarbaseProtocolError> {
    let state = match value.and_then(Value::as_str) {
        Some("open") => CoinbaseStarbaseBookState::Open,
        Some("settlement") => CoinbaseStarbaseBookState::Settlement,
        Some("delivered") => CoinbaseStarbaseBookState::Delivered,
        Some("inactive") => CoinbaseStarbaseBookState::Inactive,
        Some("locked") => CoinbaseStarbaseBookState::Locked,
        Some("halted") => CoinbaseStarbaseBookState::Halted,
        Some("archivized") => CoinbaseStarbaseBookState::Archivized,
        _ => return protocol_error(format!("Invalid {context}: unknown book state")),
    };
    Ok(state)
}

// ts: parseBaseCurrency
pub fn parse_base_currency(
    value: Option<&Value>,
    context: &str,
) -> Result<CoinbaseStarbaseBaseCurrency, CoinbaseStarbaseProtocolError> {
    base_currency(value.and_then(Value::as_str), context)
}
fn base_currency(
    value: Option<&str>,
    context: &str,
) -> Result<CoinbaseStarbaseBaseCurrency, CoinbaseStarbaseProtocolError> {
    match value {
        Some("BTC") => Ok(CoinbaseStarbaseBaseCurrency::Btc),
        Some("ETH") => Ok(CoinbaseStarbaseBaseCurrency::Eth),
        _ => protocol_error(format!("Invalid {context}: unknown base currency")),
    }
}
fn perpetual_base<'a>(name: &'a str, suffix: &str) -> Option<&'a str> {
    name.strip_suffix(suffix).filter(|base| {
        !base.is_empty()
            && base
                .bytes()
                .all(|b| b.is_ascii_uppercase() || b.is_ascii_digit())
    })
}
fn base_name(base: CoinbaseStarbaseBaseCurrency) -> &'static str {
    match base {
        CoinbaseStarbaseBaseCurrency::Btc => "BTC",
        CoinbaseStarbaseBaseCurrency::Eth => "ETH",
    }
}

// ts: parseStarbasePerpetualName
pub fn parse_starbase_perpetual_name(
    value: Option<&Value>,
) -> Result<CoinbaseStarbaseBaseCurrency, CoinbaseStarbaseProtocolError> {
    let instrument_name = require_string(value, "Starbase instrument_name")?;
    let Some(base) = perpetual_base(instrument_name, "_USDC-PERPETUAL") else {
        return protocol_error(format!(
            "Invalid Starbase perpetual instrument name: {instrument_name}"
        ));
    };
    base_currency(Some(base), "Starbase instrument_name base")
}

// ts: toCoinbaseStarbaseInstrumentName
pub fn to_coinbase_starbase_instrument_name(
    saved_symbol: &str,
) -> Result<String, CoinbaseStarbaseProtocolError> {
    if let Some(base) = perpetual_base(saved_symbol, "_USDC-PERPETUAL") {
        base_currency(Some(base), "Starbase instrument_name base")?;
        return Ok(saved_symbol.into());
    }
    let Some(base) = perpetual_base(saved_symbol, "-PERP-INTX") else {
        return protocol_error(format!(
            "Unsupported Coinbase INTX saved symbol: {saved_symbol}"
        ));
    };
    let base = base_currency(Some(base), "Coinbase INTX saved symbol base")?;
    Ok(format!("{}_USDC-PERPETUAL", base_name(base)))
}

// ts: toCoinbaseLegacyIntxSymbol
pub fn to_coinbase_legacy_intx_symbol(
    instrument_name: &str,
) -> Result<String, CoinbaseStarbaseProtocolError> {
    let base = parse_starbase_perpetual_name(Some(&Value::String(instrument_name.into())))?;
    Ok(format!("{}-PERP-INTX", base_name(base)))
}

// ts: CoinbaseStarbasePublicTicker
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePublicTicker {
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
    #[serde(rename = "timestampMs")]
    pub timestamp_ms: f64,
    pub state: CoinbaseStarbaseBookState,
    pub bid: f64,
    #[serde(rename = "bidBaseAmount")]
    pub bid_base_amount: f64,
    pub ask: f64,
    #[serde(rename = "askBaseAmount")]
    pub ask_base_amount: f64,
    pub last: f64,
    pub mark: f64,
    pub index: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub percentage: Option<f64>,
    #[serde(rename = "fundingRate", skip_serializing_if = "Option::is_none")]
    pub funding_rate: Option<f64>,
    #[serde(rename = "fundingRate8h", skip_serializing_if = "Option::is_none")]
    pub funding_rate_8h: Option<f64>,
    #[serde(rename = "volumeBase")]
    pub volume_base: f64,
    #[serde(rename = "volumeUsd", skip_serializing_if = "Option::is_none")]
    pub volume_usd: Option<f64>,
    #[serde(rename = "openInterestBase")]
    pub open_interest_base: f64,
}

// ts: normalizeCoinbaseStarbaseTicker
pub fn normalize_coinbase_starbase_ticker(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePublicTicker, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase ticker")?;
    let stats = require_record(raw.get("stats"), "Starbase ticker stats")?;
    let instrument_name = require_string(
        raw.get("instrument_name"),
        "Starbase ticker instrument_name",
    )?;
    parse_starbase_perpetual_name(Some(&Value::String(instrument_name.into())))?;
    let bid = require_positive(raw.get("best_bid_price"), "Starbase ticker best_bid_price")?;
    let ask = require_positive(raw.get("best_ask_price"), "Starbase ticker best_ask_price")?;
    let percentage = stats
        .get("price_change")
        .filter(|v| !v.is_null())
        .map(|value| {
            require_finite(
                Some(value),
                "Starbase ticker stats.price_change",
                Some(f64::NEG_INFINITY),
            )
        })
        .transpose()?;
    let volume_usd = stats
        .get("volume_usd")
        .map(|value| require_finite(Some(value), "Starbase ticker stats.volume_usd", None))
        .transpose()?;
    let funding_rate = raw
        .get("current_funding")
        .map(|value| {
            require_finite(
                Some(value),
                "Starbase ticker current_funding",
                Some(f64::NEG_INFINITY),
            )
        })
        .transpose()?;
    let funding_rate_8h = raw
        .get("funding_8h")
        .map(|value| {
            require_finite(
                Some(value),
                "Starbase ticker funding_8h",
                Some(f64::NEG_INFINITY),
            )
        })
        .transpose()?;
    Ok(CoinbaseStarbasePublicTicker {
        instrument_name: instrument_name.into(),
        legacy_saved_symbol: to_coinbase_legacy_intx_symbol(instrument_name)?,
        timestamp_ms: require_timestamp(raw.get("timestamp"), "Starbase ticker timestamp")?,
        state: parse_book_state(raw.get("state"), "Starbase ticker state")?,
        bid,
        bid_base_amount: require_finite(
            raw.get("best_bid_amount"),
            "Starbase ticker best_bid_amount",
            None,
        )?,
        ask,
        ask_base_amount: require_finite(
            raw.get("best_ask_amount"),
            "Starbase ticker best_ask_amount",
            None,
        )?,
        last: require_positive(raw.get("last_price"), "Starbase ticker last_price")?,
        mark: require_positive(raw.get("mark_price"), "Starbase ticker mark_price")?,
        index: require_positive(raw.get("index_price"), "Starbase ticker index_price")?,
        percentage,
        funding_rate,
        funding_rate_8h,
        volume_base: require_finite(stats.get("volume"), "Starbase ticker stats.volume", None)?,
        volume_usd,
        open_interest_base: require_finite(
            raw.get("open_interest"),
            "Starbase ticker open_interest",
            None,
        )?,
    })
}

// ts: normalizeCoinbaseStarbaseTickerPayload
pub fn normalize_coinbase_starbase_ticker_payload(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePublicTicker, CoinbaseStarbaseProtocolError> {
    let payload = require_record(value, "Starbase ticker subscription payload")?;
    normalize_coinbase_starbase_ticker(payload.get("data"))
}

// ts: CoinbaseStarbasePublicInstrument.quote
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseUsdc {
    #[serde(rename = "USDC")]
    Usdc,
}

// ts: CoinbaseStarbasePublicInstrument.kind
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseFuture {
    #[serde(rename = "future")]
    Future,
}

// ts: CoinbaseStarbasePublicInstrument.instrumentType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseLinear {
    #[serde(rename = "linear")]
    Linear,
}

// ts: CoinbaseStarbasePublicInstrument.settlementPeriod
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbasePerpetual {
    #[serde(rename = "perpetual")]
    Perpetual,
}

// ts: CoinbaseStarbasePublicInstrument.priceIncrementSteps
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePriceIncrementStep {
    #[serde(rename = "abovePrice")]
    pub above_price: f64,
    #[serde(rename = "priceIncrement")]
    pub price_increment: f64,
}

// ts: CoinbaseStarbasePublicInstrument
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePublicInstrument {
    #[serde(rename = "instrumentName")]
    pub instrument_name: String,
    #[serde(rename = "legacySavedSymbol")]
    pub legacy_saved_symbol: String,
    pub base: CoinbaseStarbaseBaseCurrency,
    pub quote: CoinbaseStarbaseUsdc,
    pub counter: CoinbaseStarbaseUsdc,
    pub settlement: CoinbaseStarbaseUsdc,
    pub kind: CoinbaseStarbaseFuture,
    #[serde(rename = "instrumentType")]
    pub instrument_type: CoinbaseStarbaseLinear,
    #[serde(rename = "settlementPeriod")]
    pub settlement_period: CoinbaseStarbasePerpetual,
    #[serde(rename = "isActive")]
    pub is_active: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<CoinbaseStarbaseBookState>,
    #[serde(rename = "minimumBaseAmount")]
    pub minimum_base_amount: f64,
    #[serde(rename = "contractSize")]
    pub contract_size: f64,
    #[serde(rename = "priceIncrement")]
    pub price_increment: f64,
    #[serde(rename = "priceIncrementSteps")]
    pub price_increment_steps: Vec<CoinbaseStarbasePriceIncrementStep>,
    #[serde(rename = "maximumLeverage", skip_serializing_if = "Option::is_none")]
    pub maximum_leverage: Option<f64>,
    #[serde(rename = "createdAtMs")]
    pub created_at_ms: f64,
    #[serde(rename = "expiresAtMs")]
    pub expires_at_ms: f64,
    #[serde(rename = "priceIndex")]
    pub price_index: String,
}

// ts: parseTickSizeSteps
pub fn parse_tick_size_steps(
    value: Option<&Value>,
) -> Result<Vec<CoinbaseStarbasePriceIncrementStep>, CoinbaseStarbaseProtocolError> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let Some(entries) = value.as_array() else {
        return protocol_error("Invalid Starbase instrument tick_size_steps");
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let record =
                require_record(Some(entry), &format!("Starbase tick_size_steps[{index}]"))?;
            Ok(CoinbaseStarbasePriceIncrementStep {
                above_price: require_positive(
                    record.get("above_price"),
                    &format!("Starbase tick_size_steps[{index}].above_price"),
                )?,
                price_increment: require_positive(
                    record.get("tick_size"),
                    &format!("Starbase tick_size_steps[{index}].tick_size"),
                )?,
            })
        })
        .collect()
}

// ts: normalizeCoinbaseStarbaseInstrument
pub fn normalize_coinbase_starbase_instrument(
    value: Option<&Value>,
) -> Result<CoinbaseStarbasePublicInstrument, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase instrument")?;
    if raw.get("kind").and_then(Value::as_str) != Some("future") {
        return protocol_error("Unsupported Starbase instrument kind");
    }
    if raw.get("instrument_type").and_then(Value::as_str) != Some("linear") {
        return protocol_error("Unsupported or missing Starbase instrument_type");
    }
    if raw
        .get("future_type")
        .is_some_and(|v| v.as_str() != Some("linear"))
    {
        return protocol_error("Unsupported Starbase future_type");
    }
    if raw.get("underlying_type").and_then(Value::as_str) != Some("crypto") {
        return protocol_error("Unsupported or missing Starbase underlying_type");
    }
    if raw.contains_key("option_type") {
        return protocol_error("Unsupported Starbase option_type");
    }
    if raw.get("settlement_period").and_then(Value::as_str) != Some("perpetual") {
        return protocol_error("Unsupported Starbase settlement_period");
    }
    if raw.get("quote_currency").and_then(Value::as_str) != Some("USDC") {
        return protocol_error("Unsupported Starbase quote_currency");
    }
    if raw.get("counter_currency").and_then(Value::as_str) != Some("USDC") {
        return protocol_error("Unsupported Starbase counter_currency");
    }
    if raw.get("settlement_currency").and_then(Value::as_str) != Some("USDC") {
        return protocol_error("Unsupported Starbase settlement_currency");
    }
    let Some(is_active) = raw.get("is_active").and_then(Value::as_bool) else {
        return protocol_error("Invalid Starbase instrument is_active");
    };
    let base = parse_base_currency(
        raw.get("base_currency"),
        "Starbase instrument base_currency",
    )?;
    let instrument_name = require_string(raw.get("instrument_name"), "Starbase instrument_name")?;
    if parse_starbase_perpetual_name(raw.get("instrument_name"))? != base {
        return protocol_error("Starbase instrument_name does not match base_currency");
    }
    let maximum_leverage = raw
        .get("max_leverage")
        .map(|v| require_positive_integer(Some(v), "Starbase instrument max_leverage"))
        .transpose()?;
    Ok(CoinbaseStarbasePublicInstrument {
        instrument_name: instrument_name.into(),
        legacy_saved_symbol: to_coinbase_legacy_intx_symbol(instrument_name)?,
        base,
        quote: CoinbaseStarbaseUsdc::Usdc,
        counter: CoinbaseStarbaseUsdc::Usdc,
        settlement: CoinbaseStarbaseUsdc::Usdc,
        kind: CoinbaseStarbaseFuture::Future,
        instrument_type: CoinbaseStarbaseLinear::Linear,
        settlement_period: CoinbaseStarbasePerpetual::Perpetual,
        is_active,
        state: raw
            .get("state")
            .map(|v| parse_book_state(Some(v), "Starbase instrument state"))
            .transpose()?,
        minimum_base_amount: require_positive(
            raw.get("min_trade_amount"),
            "Starbase instrument min_trade_amount",
        )?,
        contract_size: require_positive(
            raw.get("contract_size"),
            "Starbase instrument contract_size",
        )?,
        price_increment: require_positive(raw.get("tick_size"), "Starbase instrument tick_size")?,
        price_increment_steps: parse_tick_size_steps(raw.get("tick_size_steps"))?,
        maximum_leverage,
        created_at_ms: require_timestamp(
            raw.get("creation_timestamp"),
            "Starbase instrument creation_timestamp",
        )?,
        expires_at_ms: require_timestamp(
            raw.get("expiration_timestamp"),
            "Starbase instrument expiration_timestamp",
        )?,
        price_index: require_string(raw.get("price_index"), "Starbase instrument price_index")?
            .into(),
    })
}

// ts: normalizeCoinbaseStarbaseInstruments
pub fn normalize_coinbase_starbase_instruments(
    value: Option<&Value>,
) -> Result<Vec<CoinbaseStarbasePublicInstrument>, CoinbaseStarbaseProtocolError> {
    let Some(entries) = value.and_then(Value::as_array) else {
        return protocol_error("Invalid public/get_instruments result");
    };
    let raw_instruments = entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            require_record(
                Some(entry),
                &format!("public/get_instruments result[{index}]"),
            )
        })
        .collect::<Result<Vec<_>, _>>()?;
    let instruments = raw_instruments
        .into_iter()
        .filter(|instrument| {
            matches!(
                instrument.get("instrument_name").and_then(Value::as_str),
                Some("BTC_USDC-PERPETUAL" | "ETH_USDC-PERPETUAL")
            )
        })
        .map(|instrument| {
            normalize_coinbase_starbase_instrument(Some(&Value::Object(instrument.clone())))
        })
        .collect::<Result<Vec<_>, _>>()?;
    let mut names = std::collections::BTreeSet::new();
    for instrument in &instruments {
        if !names.insert(instrument.instrument_name.as_str()) {
            return protocol_error("Duplicate Starbase instrument_name");
        }
    }
    if ["BTC_USDC-PERPETUAL", "ETH_USDC-PERPETUAL"]
        .iter()
        .any(|name| !names.contains(name))
    {
        return protocol_error("Missing supported Starbase perpetual instrument");
    }
    Ok(instruments)
}

// ts: parseRestBookLevels
pub fn parse_rest_book_levels(
    value: Option<&Value>,
    context: &str,
) -> Result<Vec<safe_cex_types::OrderBookOrders>, CoinbaseStarbaseProtocolError> {
    let Some(entries) = value.and_then(Value::as_array) else {
        return protocol_error(format!("Invalid {context}: expected an array"));
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let Some(level) = entry.as_array().filter(|level| level.len() == 2) else {
                return protocol_error(format!("Invalid {context}[{index}]"));
            };
            Ok(safe_cex_types::OrderBookOrders {
                price: require_positive(level.first(), &format!("{context}[{index}].price"))?,
                amount: require_finite(level.get(1), &format!("{context}[{index}].amount"), None)?,
                total: 0.0,
            })
        })
        .collect()
}

// ts: normalizeCoinbaseStarbaseOrderBookSnapshot
pub fn normalize_coinbase_starbase_order_book_snapshot(
    value: Option<&Value>,
) -> Result<safe_cex_types::OrderBook, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase order book")?;
    let mut book = safe_cex_types::OrderBook {
        bids: parse_rest_book_levels(raw.get("bids"), "Starbase order book bids")?,
        asks: parse_rest_book_levels(raw.get("asks"), "Starbase order book asks")?,
    };
    safe_cex_store::orderbook::sort_order_book(&mut book);
    safe_cex_store::orderbook::calc_order_book_total(&mut book);
    Ok(book)
}

// ts: parseCandle
pub fn parse_candle(
    raw: &serde_json::Map<String, Value>,
    context: &str,
) -> Result<safe_cex_types::Candle, CoinbaseStarbaseProtocolError> {
    Ok(safe_cex_types::Candle {
        timestamp: require_timestamp(raw.get("tick"), &format!("{context}.tick"))? / 1000.0,
        open: require_positive(raw.get("open"), &format!("{context}.open"))?,
        high: require_positive(raw.get("high"), &format!("{context}.high"))?,
        low: require_positive(raw.get("low"), &format!("{context}.low"))?,
        close: require_positive(raw.get("close"), &format!("{context}.close"))?,
        volume: require_finite(raw.get("volume"), &format!("{context}.volume"), None)?,
        last_side: None,
    })
}

// ts: normalizeCoinbaseStarbaseChartPayload
pub fn normalize_coinbase_starbase_chart_payload(
    value: Option<&Value>,
) -> Result<safe_cex_types::Candle, CoinbaseStarbaseProtocolError> {
    let payload = require_record(value, "Starbase chart subscription payload")?;
    parse_candle(
        require_record(payload.get("data"), "Starbase chart subscription data")?,
        "Starbase chart data",
    )
}

// ts: normalizeCoinbaseStarbaseChartData
pub fn normalize_coinbase_starbase_chart_data(
    value: Option<&Value>,
) -> Result<Vec<safe_cex_types::Candle>, CoinbaseStarbaseProtocolError> {
    let raw = require_record(value, "Starbase chart data")?;
    if raw.get("status").and_then(Value::as_str) == Some("no_data") {
        return Ok(Vec::new());
    }
    if raw.get("status").and_then(Value::as_str) != Some("ok") {
        return protocol_error("Invalid Starbase chart status");
    }
    let fields = ["ticks", "open", "high", "low", "close", "volume"];
    let mut arrays = Vec::with_capacity(fields.len());
    for field in fields {
        let Some(entries) = raw.get(field).and_then(Value::as_array) else {
            return protocol_error(format!("Invalid Starbase chart {field}"));
        };
        arrays.push(entries);
    }
    if arrays
        .iter()
        .any(|entries| entries.len() != arrays[0].len())
    {
        return protocol_error(
            "Invalid Starbase chart data: parallel arrays have different lengths",
        );
    }
    (0..arrays[0].len())
        .map(|index| {
            let record = ["tick", "open", "high", "low", "close", "volume"]
                .into_iter()
                .zip(&arrays)
                .map(|(field, entries)| (field.into(), entries[index].clone()))
                .collect();
            parse_candle(&record, &format!("Starbase chart data[{index}]"))
        })
        .collect()
}

// ts: CoinbaseStarbaseOrderBookDepth
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[repr(u32)]
#[serde(into = "u32")]
pub enum CoinbaseStarbaseOrderBookDepth {
    D1 = 1,
    D5 = 5,
    D10 = 10,
    D20 = 20,
    D50 = 50,
    D100 = 100,
    D1000 = 1000,
    D10000 = 10000,
}
impl From<CoinbaseStarbaseOrderBookDepth> for u32 {
    fn from(value: CoinbaseStarbaseOrderBookDepth) -> Self {
        value as Self
    }
}

// ts: CoinbaseStarbasePublicRequest
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbasePublicRequest<
    Method: AsRef<str>,
    Params: Into<serde_json::Map<String, Value>>,
> {
    pub method: Method,
    pub params: Params,
}

// ts: parseDeltaLevels (helper-local StarbaseBookAction)
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum StarbaseBookAction {
    #[serde(rename = "new")]
    New,
    #[serde(rename = "change")]
    Change,
    #[serde(rename = "delete")]
    Delete,
}

// ts: parseDeltaLevels.frameType
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseBookFrameType {
    #[serde(rename = "snapshot")]
    Snapshot,
    #[serde(rename = "change")]
    Change,
}

// ts: parseDeltaLevels (helper-local ParsedStarbaseBookLevel)
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ParsedStarbaseBookLevel {
    pub action: StarbaseBookAction,
    pub amount: f64,
    pub price: f64,
}

// ts: parseDeltaLevels
pub fn parse_delta_levels(
    value: Option<&Value>,
    frame_type: CoinbaseStarbaseBookFrameType,
    context: &str,
) -> Result<Vec<ParsedStarbaseBookLevel>, CoinbaseStarbaseProtocolError> {
    let Some(entries) = value.and_then(Value::as_array) else {
        return protocol_error(format!("Invalid {context}: expected an array"));
    };
    entries
        .iter()
        .enumerate()
        .map(|(index, entry)| {
            let Some(level) = entry.as_array().filter(|level| level.len() == 3) else {
                return protocol_error(format!("Invalid {context}[{index}]"));
            };
            let action = match level.first().and_then(Value::as_str) {
                Some("new") => StarbaseBookAction::New,
                Some("change") => StarbaseBookAction::Change,
                Some("delete") => StarbaseBookAction::Delete,
                _ => return protocol_error(format!("Invalid {context}[{index}] action")),
            };
            if frame_type == CoinbaseStarbaseBookFrameType::Snapshot
                && action != StarbaseBookAction::New
            {
                return protocol_error(format!("Invalid {context}[{index}] snapshot action"));
            }
            let price = require_positive(level.get(1), &format!("{context}[{index}].price"))?;
            let amount = require_finite(level.get(2), &format!("{context}[{index}].amount"), None)?;
            if action == StarbaseBookAction::Delete && amount != 0.0 {
                return protocol_error(format!("Invalid {context}[{index}] delete amount"));
            }
            if action != StarbaseBookAction::Delete && amount == 0.0 {
                let action_name = match action {
                    StarbaseBookAction::New => "new",
                    StarbaseBookAction::Change => "change",
                    StarbaseBookAction::Delete => "delete",
                };
                return protocol_error(format!("Invalid {context}[{index}] {action_name} amount"));
            }
            Ok(ParsedStarbaseBookLevel {
                action,
                amount,
                price,
            })
        })
        .collect()
}

// ts: buildCoinbaseStarbaseGetInstruments
pub fn build_coinbase_starbase_get_instruments()
-> CoinbaseStarbasePublicRequest<&'static str, serde_json::Map<String, Value>> {
    CoinbaseStarbasePublicRequest {
        method: "public/get_instruments",
        params: serde_json::Map::from_iter([
            ("currency".into(), Value::String("USDC".into())),
            ("expired".into(), Value::Bool(false)),
            ("kind".into(), Value::String("future".into())),
        ]),
    }
}

// ts: buildCoinbaseStarbaseTicker
pub fn build_coinbase_starbase_ticker(
    symbol: &str,
) -> Result<
    CoinbaseStarbasePublicRequest<&'static str, serde_json::Map<String, Value>>,
    CoinbaseStarbaseProtocolError,
> {
    Ok(CoinbaseStarbasePublicRequest {
        method: "public/ticker",
        params: serde_json::Map::from_iter([(
            "instrument_name".into(),
            Value::String(to_coinbase_starbase_instrument_name(symbol)?),
        )]),
    })
}

// ts: buildCoinbaseStarbaseOrderBook
pub fn build_coinbase_starbase_order_book(
    symbol: &str,
    depth: Option<&Value>,
) -> Result<
    CoinbaseStarbasePublicRequest<&'static str, serde_json::Map<String, Value>>,
    CoinbaseStarbaseProtocolError,
> {
    let Some(depth) = depth
        .and_then(Value::as_f64)
        .filter(|depth| [1.0, 5.0, 10.0, 20.0, 50.0, 100.0, 1000.0, 10000.0].contains(depth))
    else {
        return Err(CoinbaseStarbaseProtocolError::new(
            "Unsupported Starbase order book depth",
        ));
    };
    Ok(CoinbaseStarbasePublicRequest {
        method: "public/get_order_book",
        params: serde_json::Map::from_iter([
            ("depth".into(), serde_json::json!(depth as u32)),
            (
                "instrument_name".into(),
                Value::String(to_coinbase_starbase_instrument_name(symbol)?),
            ),
        ]),
    })
}

// ts: CoinbaseStarbaseMarketParityResult.status
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseMarketParityStatus {
    #[serde(rename = "blocked")]
    Blocked,
}

// ts: CoinbaseStarbaseMarketParityResult
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct CoinbaseStarbaseMarketParityResult<'a> {
    pub status: CoinbaseStarbaseMarketParityStatus,
    pub gateway: &'a CoinbaseStarbasePublicInstrument,
    pub blockers: Vec<CoinbaseStarbaseMarketParityBlocker>,
}

// ts: assessCoinbaseStarbaseMarketParity
pub fn assess_coinbase_starbase_market_parity(
    gateway: &CoinbaseStarbasePublicInstrument,
) -> CoinbaseStarbaseMarketParityResult<'_> {
    CoinbaseStarbaseMarketParityResult {
        status: CoinbaseStarbaseMarketParityStatus::Blocked,
        gateway,
        blockers: vec![
            CoinbaseStarbaseMarketParityBlocker::AmountIncrementUnpublished,
            CoinbaseStarbaseMarketParityBlocker::MaximumBaseAmountUnpublished,
        ],
    }
}

// ts: resolveChartResolution
// ts-bug: resolveChartResolution accepts inherited Object.prototype members as
// resolutions. CORRECTED: only declared own timeframe entries are supported.
pub fn resolve_chart_resolution(
    timeframe: &str,
) -> Result<&'static str, CoinbaseStarbaseProtocolError> {
    crate::protocol::COINBASE_STARBASE_TIMEFRAME_RESOLUTION
        .iter()
        .find_map(|(key, resolution)| (*key == timeframe).then_some(*resolution))
        .ok_or_else(|| {
            CoinbaseStarbaseProtocolError::new(format!(
                "Unsupported Starbase chart timeframe: {timeframe}"
            ))
        })
}

// ts: buildCoinbaseStarbaseChartData
pub fn build_coinbase_starbase_chart_data(
    symbol: &str,
    timeframe: &str,
    start_timestamp: Option<&Value>,
    end_timestamp: Option<&Value>,
) -> Result<
    CoinbaseStarbasePublicRequest<&'static str, serde_json::Map<String, Value>>,
    CoinbaseStarbaseProtocolError,
> {
    let start = require_timestamp(start_timestamp, "Starbase chart start_timestamp")?;
    let end = require_timestamp(end_timestamp, "Starbase chart end_timestamp")?;
    if end < start {
        return protocol_error("Starbase chart end_timestamp precedes start_timestamp");
    }
    let instrument_name = to_coinbase_starbase_instrument_name(symbol)?;
    let resolution = resolve_chart_resolution(timeframe)?;
    Ok(CoinbaseStarbasePublicRequest {
        method: "public/get_tradingview_chart_data",
        params: serde_json::Map::from_iter([
            ("end_timestamp".into(), serde_json::json!(end)),
            ("instrument_name".into(), Value::String(instrument_name)),
            ("resolution".into(), Value::String(resolution.into())),
            ("start_timestamp".into(), serde_json::json!(start)),
        ]),
    })
}

// ts: CoinbaseStarbaseSequencedOrderBook
pub struct CoinbaseStarbaseSequencedOrderBook {
    order_book: std::sync::Arc<std::sync::Mutex<safe_cex_types::OrderBook>>,
    awaiting_snapshot: bool,
    has_snapshot: bool,
    sequenced_book: safe_cex_store::sequenced_delta_order_book::SequencedDeltaOrderBook,
}
impl Default for CoinbaseStarbaseSequencedOrderBook {
    fn default() -> Self {
        Self::new()
    }
}
impl CoinbaseStarbaseSequencedOrderBook {
    // ts: CoinbaseStarbaseSequencedOrderBook field initializers
    pub fn new() -> Self {
        let order_book = std::sync::Arc::new(std::sync::Mutex::new(safe_cex_types::OrderBook {
            bids: Vec::new(),
            asks: Vec::new(),
        }));
        Self {
            sequenced_book:
                safe_cex_store::sequenced_delta_order_book::SequencedDeltaOrderBook::new(
                    order_book.clone(),
                ),
            order_book,
            awaiting_snapshot: false,
            has_snapshot: false,
        }
    }
    // ts: CoinbaseStarbaseSequencedOrderBook.orderBook
    pub fn order_book(&self) -> std::sync::Arc<std::sync::Mutex<safe_cex_types::OrderBook>> {
        self.order_book.clone()
    }
    // ts: CoinbaseStarbaseSequencedOrderBook.applyPayload
    pub fn apply_payload(
        &mut self,
        value: Option<&Value>,
    ) -> Result<
        safe_cex_store::sequenced_delta_order_book::SequencedBookResult,
        CoinbaseStarbaseProtocolError,
    > {
        use safe_cex_store::sequenced_delta_order_book::{
            OrderBookSequence, SequencedBookDelta, SequencedBookFrame, SequencedBookIgnoreReason,
            SequencedBookRecoveryReason, SequencedBookResult,
        };
        let payload = require_record(value, "Starbase book subscription payload")?;
        let data = require_record(payload.get("data"), "Starbase book subscription data")?;
        let frame_type = match data.get("type").and_then(Value::as_str) {
            Some("snapshot") => CoinbaseStarbaseBookFrameType::Snapshot,
            Some("change") => CoinbaseStarbaseBookFrameType::Change,
            _ => return protocol_error("Invalid Starbase book subscription type"),
        };
        parse_starbase_perpetual_name(data.get("instrument_name"))?;
        if self.awaiting_snapshot && frame_type == CoinbaseStarbaseBookFrameType::Change {
            return Ok(SequencedBookResult::Ignored {
                reason: SequencedBookIgnoreReason::AwaitingSnapshot,
                sequence: None,
            });
        }
        let sequence = require_timestamp(data.get("change_id"), "Starbase book change_id")?;
        let bids = parse_delta_levels(data.get("bids"), frame_type, "Starbase book bids")?;
        let asks = parse_delta_levels(data.get("asks"), frame_type, "Starbase book asks")?;
        let previous_sequence = if frame_type == CoinbaseStarbaseBookFrameType::Change {
            Some(require_timestamp(
                data.get("prev_change_id"),
                "Starbase book prev_change_id",
            )?)
        } else {
            None
        };
        let bid_levels = bids
            .iter()
            .map(|level| vec![level.price, level.amount])
            .collect();
        let ask_levels = asks
            .iter()
            .map(|level| vec![level.price, level.amount])
            .collect();
        if let Some(previous_sequence) = previous_sequence.filter(|_| !self.has_snapshot) {
            return Ok(self.sequenced_book.apply_delta(&SequencedBookDelta {
                sequence: OrderBookSequence::Number(sequence),
                previous_sequence: OrderBookSequence::Number(previous_sequence),
                bids: bid_levels,
                asks: ask_levels,
            }));
        }
        // ts-bug: CoinbaseStarbaseSequencedOrderBook.applyPayload checks actions
        // before shared duplicate/stale filtering. Replaying a valid new-level
        // delta therefore clears the book as invalid-level instead of ignoring it.
        if !self.actions_match_book(true, &bids, frame_type)
            || !self.actions_match_book(false, &asks, frame_type)
        {
            // No book lock is held across calls into the shared state machine.
            self.sequenced_book.reset();
            self.awaiting_snapshot = true;
            self.has_snapshot = false;
            return Ok(SequencedBookResult::Recover {
                reason: SequencedBookRecoveryReason::InvalidLevel,
            });
        }
        let result = if let Some(previous_sequence) = previous_sequence {
            self.sequenced_book.apply_delta(&SequencedBookDelta {
                sequence: OrderBookSequence::Number(sequence),
                previous_sequence: OrderBookSequence::Number(previous_sequence),
                bids: bid_levels,
                asks: ask_levels,
            })
        } else {
            self.sequenced_book.apply_snapshot(&SequencedBookFrame {
                sequence: OrderBookSequence::Number(sequence),
                bids: bid_levels,
                asks: ask_levels,
            })
        };
        match &result {
            SequencedBookResult::Applied { .. } => {
                self.awaiting_snapshot = false;
                self.has_snapshot = true;
                let mut book = self
                    .order_book
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                safe_cex_store::orderbook::calc_order_book_total(&mut book);
            }
            SequencedBookResult::Recover { .. } => self.has_snapshot = false,
            SequencedBookResult::Ignored { .. } => {}
        }
        Ok(result)
    }
    // ts: CoinbaseStarbaseSequencedOrderBook.onReconnect
    pub fn on_reconnect(&mut self) {
        self.sequenced_book.reset();
        self.awaiting_snapshot = false;
        self.has_snapshot = false;
    }
    // ts: CoinbaseStarbaseSequencedOrderBook#actionsMatchBook
    fn actions_match_book(
        &self,
        bids: bool,
        levels: &[ParsedStarbaseBookLevel],
        frame_type: CoinbaseStarbaseBookFrameType,
    ) -> bool {
        // Parsed query prices are finite and positive; bit identity is JS Set
        // membership on this boundary. The set is never iterated for output.
        let mut prices: std::collections::HashSet<u64> =
            if frame_type == CoinbaseStarbaseBookFrameType::Snapshot {
                std::collections::HashSet::new()
            } else {
                let book = self
                    .order_book
                    .lock()
                    .unwrap_or_else(|error| error.into_inner());
                let side = if bids { &book.bids } else { &book.asks };
                side.iter().map(|level| level.price.to_bits()).collect()
            };
        for level in levels {
            let exists = prices.contains(&level.price.to_bits());
            if level.action == StarbaseBookAction::New {
                if exists {
                    return false;
                }
                prices.insert(level.price.to_bits());
            } else {
                if !exists {
                    return false;
                }
                if level.action == StarbaseBookAction::Delete {
                    prices.remove(&level.price.to_bits());
                }
            }
        }
        true
    }
}

// ts: buildCoinbaseStarbaseTickerChannel
pub fn build_coinbase_starbase_ticker_channel(
    symbol: &str,
    interval: Option<&Value>,
) -> Result<CoinbaseStarbasePublicChannel, CoinbaseStarbaseProtocolError> {
    let interval = match interval {
        None => CoinbaseStarbaseBookInterval::V100Ms,
        Some(Value::String(value)) if value == "100ms" => CoinbaseStarbaseBookInterval::V100Ms,
        Some(Value::String(value)) if value == "agg2" => CoinbaseStarbaseBookInterval::Agg2,
        _ => return protocol_error("Unsupported Starbase ticker interval"),
    };
    Ok(CoinbaseStarbasePublicChannel::Ticker {
        instrument_name: to_coinbase_starbase_instrument_name(symbol)?,
        interval,
    })
}

// ts: buildCoinbaseStarbaseBookChannel
pub fn build_coinbase_starbase_book_channel(
    symbol: &str,
    interval: Option<&Value>,
) -> Result<CoinbaseStarbasePublicChannel, CoinbaseStarbaseProtocolError> {
    let interval = match interval {
        None => CoinbaseStarbaseBookInterval::V100Ms,
        Some(Value::String(value)) if value == "100ms" => CoinbaseStarbaseBookInterval::V100Ms,
        Some(Value::String(value)) if value == "agg2" => CoinbaseStarbaseBookInterval::Agg2,
        _ => return protocol_error("Unsupported Starbase book interval"),
    };
    Ok(CoinbaseStarbasePublicChannel::Book {
        instrument_name: to_coinbase_starbase_instrument_name(symbol)?,
        interval,
    })
}

// ts: buildCoinbaseStarbaseChartChannel
// ts-bug: inherited resolution values are coerced into channel strings in TS.
// CORRECTED: retain the own-key-only policy of resolve_chart_resolution.
pub fn build_coinbase_starbase_chart_channel(
    symbol: &str,
    timeframe: &str,
) -> Result<CoinbaseStarbasePublicChannel, CoinbaseStarbaseProtocolError> {
    let instrument_name = to_coinbase_starbase_instrument_name(symbol)?;
    let resolution = match resolve_chart_resolution(timeframe)? {
        "1" => CoinbaseStarbaseChartResolution::V1,
        "3" => CoinbaseStarbaseChartResolution::V3,
        "5" => CoinbaseStarbaseChartResolution::V5,
        "15" => CoinbaseStarbaseChartResolution::V15,
        "30" => CoinbaseStarbaseChartResolution::V30,
        "60" => CoinbaseStarbaseChartResolution::V60,
        "120" => CoinbaseStarbaseChartResolution::V120,
        "360" => CoinbaseStarbaseChartResolution::V360,
        "720" => CoinbaseStarbaseChartResolution::V720,
        "1D" => CoinbaseStarbaseChartResolution::V1D,
        _ => return protocol_error("Unsupported Starbase chart resolution"),
    };
    Ok(CoinbaseStarbasePublicChannel::Chart {
        instrument_name,
        resolution,
    })
}

// ts: buildCoinbaseStarbaseInstrumentStateChannel
pub fn build_coinbase_starbase_instrument_state_channel(
    currency: Option<&Value>,
) -> Result<CoinbaseStarbasePublicChannel, CoinbaseStarbaseProtocolError> {
    let currency = match currency.and_then(Value::as_str) {
        Some("BTC") => CoinbaseStarbaseInstrumentStateCurrency::Btc,
        Some("ETH") => CoinbaseStarbaseInstrumentStateCurrency::Eth,
        Some("USDC") => CoinbaseStarbaseInstrumentStateCurrency::Usdc,
        Some("any") => CoinbaseStarbaseInstrumentStateCurrency::Any,
        _ => return protocol_error("Unsupported Starbase instrument-state currency"),
    };
    Ok(CoinbaseStarbasePublicChannel::InstrumentState { currency })
}

type SubscriptionRequest =
    CoinbaseStarbasePublicRequest<&'static str, serde_json::Map<String, Value>>;

// Method constraint of buildSubscriptionRequest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub enum CoinbaseStarbaseSubscriptionMethod {
    #[serde(rename = "public/subscribe")]
    Subscribe,
    #[serde(rename = "public/unsubscribe")]
    Unsubscribe,
}

// ts: buildSubscriptionRequest
pub fn build_subscription_request(
    method: CoinbaseStarbaseSubscriptionMethod,
    channels: &[CoinbaseStarbasePublicChannel],
) -> Result<SubscriptionRequest, CoinbaseStarbaseProtocolError> {
    let method = match method {
        CoinbaseStarbaseSubscriptionMethod::Subscribe => "public/subscribe",
        CoinbaseStarbaseSubscriptionMethod::Unsubscribe => "public/unsubscribe",
    };
    if channels.is_empty()
        || channels
            .iter()
            .enumerate()
            .any(|(index, channel)| channels[..index].contains(channel))
    {
        return protocol_error(format!("Invalid {method} channels"));
    }
    let channels = serde_json::to_value(channels)
        .map_err(|error| CoinbaseStarbaseProtocolError::new(error.to_string()))?;
    Ok(CoinbaseStarbasePublicRequest {
        method,
        params: serde_json::Map::from_iter([("channels".into(), channels)]),
    })
}

// ts: CoinbaseStarbasePublicSubscriptions
#[derive(Default)]
pub struct CoinbaseStarbasePublicSubscriptions {
    // An insertion-ordered set. Removal followed by addition moves to the end.
    desired_channels: Vec<CoinbaseStarbasePublicChannel>,
}

impl CoinbaseStarbasePublicSubscriptions {
    /// Mirrors the implicit TypeScript constructor's empty desired-channel set.
    pub fn new() -> Self {
        Self::default()
    }

    // ts: CoinbaseStarbasePublicSubscriptions.subscribe
    pub fn subscribe(
        &mut self,
        channels: &[CoinbaseStarbasePublicChannel],
    ) -> Result<SubscriptionRequest, CoinbaseStarbaseProtocolError> {
        let request =
            build_subscription_request(CoinbaseStarbaseSubscriptionMethod::Subscribe, channels)?;
        for channel in channels {
            if !self.desired_channels.contains(channel) {
                self.desired_channels.push(channel.clone());
            }
        }
        Ok(request)
    }

    // ts: CoinbaseStarbasePublicSubscriptions.unsubscribe
    pub fn unsubscribe(
        &mut self,
        channels: &[CoinbaseStarbasePublicChannel],
    ) -> Result<SubscriptionRequest, CoinbaseStarbaseProtocolError> {
        let request =
            build_subscription_request(CoinbaseStarbaseSubscriptionMethod::Unsubscribe, channels)?;
        self.desired_channels
            .retain(|channel| !channels.contains(channel));
        Ok(request)
    }

    // ts: CoinbaseStarbasePublicSubscriptions.buildReconnectRequest
    pub fn build_reconnect_request(
        &self,
    ) -> Result<Option<SubscriptionRequest>, CoinbaseStarbaseProtocolError> {
        if self.desired_channels.is_empty() {
            return Ok(None);
        }
        build_subscription_request(
            CoinbaseStarbaseSubscriptionMethod::Subscribe,
            &self.desired_channels,
        )
        .map(Some)
    }

    // ts: CoinbaseStarbasePublicSubscriptions.validateAcknowledgement
    // ts-bug: repeated requested channels do not require repeated acknowledgement;
    // requested [a,a] accepts [a,b] because only length and membership are checked.
    pub fn validate_acknowledgement(
        &self,
        value: Option<&Value>,
        requested: &[CoinbaseStarbasePublicChannel],
    ) -> Result<(), CoinbaseStarbaseProtocolError> {
        let Some(acknowledged) = value
            .and_then(Value::as_array)
            .filter(|values| values.iter().all(Value::is_string))
        else {
            return protocol_error("Invalid Starbase subscription acknowledgement");
        };
        if acknowledged.len() != requested.len() {
            return protocol_error("Incomplete Starbase subscription acknowledgement");
        }
        for channel in requested {
            let channel = serde_json::to_value(channel)
                .map_err(|error| CoinbaseStarbaseProtocolError::new(error.to_string()))?;
            if !acknowledged.contains(&channel) {
                return protocol_error("Incomplete Starbase subscription acknowledgement");
            }
        }
        Ok(())
    }
}
