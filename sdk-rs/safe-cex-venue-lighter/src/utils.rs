use safe_cex_num::{PrecisionInput, js_number_string, number_from_string};

// ts: getPrecision
// ts-bug: getPrecision counts positive-exponent suffixes and trailing whitespace
// as decimal digits, and uses only the exponent for negative scientific notation.
pub fn get_precision(num: impl Into<PrecisionInput>) -> f64 {
    let (value, text) = match num.into() {
        PrecisionInput::Number(value) => (value, js_number_string(value)),
        PrecisionInput::String(text) => (number_from_string(&text), text),
    };
    if value.is_nan() {
        return 0.0;
    }
    let text = text.to_lowercase();
    if let Some(exponent) = text.split("e-").nth(1) {
        let exponent = number_from_string(exponent);
        return if exponent.is_finite() { exponent } else { 0.0 };
    }
    let mut parts = text.split('.');
    parts.next();
    match (parts.next(), parts.next()) {
        (Some(tail), None) => tail.encode_utf16().count() as f64,
        _ => 0.0,
    }
}

// ts: dropPrefix
pub fn drop_prefix(hash: &str) -> &str {
    hash.strip_prefix("0x").unwrap_or(hash)
}

// ts: addPrefix
pub fn add_prefix(hash: &str) -> String {
    if hash.starts_with("0x") {
        hash.to_owned()
    } else {
        format!("0x{hash}")
    }
}

// ts: clampDecimals
pub fn clamp_decimals(decimals: f64) -> f64 {
    if !decimals.is_finite() {
        return 18.0;
    }
    decimals.trunc().clamp(0.0, 30.0).max(0.0)
}

// ts: LighterCancelTarget
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterCancelTarget {
    pub order_id: String,
    pub market_id: f64,
}

// ts: LighterOrderMinimums
#[derive(Debug, Clone, PartialEq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LighterOrderMinimums {
    pub min_base_amount: f64,
    pub min_quote_amount: f64,
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterCancelTarget", rename_all = "camelCase")]
struct LighterCancelTargetWire {
    order_id: String,
    market_id: f64,
}
impl<'de> serde::Deserialize<'de> for LighterCancelTarget {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                LighterCancelTargetWire::deserialize(safe_cex_types::strict_decode::object(decoder))
            })
            .map_err(serde::de::Error::custom)
    }
}

// A TS object record has named fields; serde's positional sequence form is wider.
#[derive(serde::Deserialize)]
#[serde(remote = "LighterOrderMinimums", rename_all = "camelCase")]
struct LighterOrderMinimumsWire {
    min_base_amount: f64,
    min_quote_amount: f64,
}
impl<'de> serde::Deserialize<'de> for LighterOrderMinimums {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let input =
            <safe_cex_num::numeric_ingress::JsNumberInput as serde::Deserialize>::deserialize(
                deserializer,
            )?;
        input
            .decode_with(|decoder| {
                LighterOrderMinimumsWire::deserialize(safe_cex_types::strict_decode::object(
                    decoder,
                ))
            })
            .map_err(serde::de::Error::custom)
    }
}

// ts: parseLighterApiKeyIndex
pub fn parse_lighter_api_key_index(
    value: Option<&serde_json::Value>,
) -> Result<Option<f64>, safe_cex_num::JsCoercionError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    if let Some(number) = value.as_f64().filter(|number| number.is_finite()) {
        return Ok(Some(number.trunc()));
    }
    let text = safe_cex_num::js_string(Some(value))?;
    let text = safe_cex_num::js_trim(&text);
    let Some(first) = text.bytes().position(|byte| byte.is_ascii_digit()) else {
        return Ok(None);
    };
    let start = if first > 0 && text.as_bytes()[first - 1] == b'-' {
        first - 1
    } else {
        first
    };
    let digits = text[first..].bytes().take_while(u8::is_ascii_digit).count();
    let parsed = safe_cex_num::parse_int(&text[start..first + digits], Some(10));
    Ok(parsed.is_finite().then_some(parsed))
}

// ts: toLighterApiKeyIndexString
pub fn to_lighter_api_key_index_string(
    value: Option<&serde_json::Value>,
    fallback: Option<&str>,
) -> Result<String, safe_cex_num::JsCoercionError> {
    Ok(parse_lighter_api_key_index(value)?
        .map(js_number_string)
        .unwrap_or_else(|| fallback.unwrap_or("0").to_owned()))
}

// ts: parseLighterPositiveIntegerString
pub fn parse_lighter_positive_integer_string(
    value: Option<&serde_json::Value>,
) -> Result<Option<String>, safe_cex_num::JsCoercionError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(None);
    };
    let raw = safe_cex_num::js_string(Some(value))?;
    let raw = safe_cex_num::js_trim(&raw);
    if raw.is_empty() || !raw.bytes().all(|byte| byte.is_ascii_digit()) {
        return Ok(None);
    }
    let normalized = raw.trim_start_matches('0');
    Ok(Some(
        if normalized.is_empty() {
            "0"
        } else {
            normalized
        }
        .to_owned(),
    ))
}

// ts: isLighterL1Address
pub fn is_lighter_l1_address(
    value: Option<&serde_json::Value>,
) -> Result<bool, safe_cex_num::JsCoercionError> {
    let Some(value) = value.filter(|value| !value.is_null()) else {
        return Ok(false);
    };
    let raw = safe_cex_num::js_string(Some(value))?;
    let raw = safe_cex_num::js_trim(&raw);
    Ok(raw.len() == 42
        && raw.starts_with("0x")
        && raw[2..].bytes().all(|byte| byte.is_ascii_hexdigit()))
}

// ts: normalizeNonNegativeNumber
pub fn normalize_non_negative_number(
    value: Option<&serde_json::Value>,
) -> Result<f64, safe_cex_num::JsCoercionError> {
    let text = match value.filter(|value| !value.is_null()) {
        Some(value) => safe_cex_num::js_string(Some(value))?,
        None => String::new(),
    };
    let parsed = safe_cex_num::parse_float(&text);
    Ok(if !parsed.is_finite() || parsed < 0.0 {
        0.0
    } else {
        parsed
    })
}

// ts: getLighterOrderMinimums
pub fn get_lighter_order_minimums(
    market: &safe_cex_types::Market,
) -> Result<LighterOrderMinimums, safe_cex_num::JsCoercionError> {
    let info = market.info.as_ref();
    let base = info
        .and_then(|info| info.get("min_base_amount"))
        .filter(|value| !value.is_null())
        .or_else(|| info.and_then(|info| info.get("minBaseAmount")));
    let quote = info
        .and_then(|info| info.get("min_quote_amount"))
        .filter(|value| !value.is_null())
        .or_else(|| info.and_then(|info| info.get("minQuoteAmount")));
    Ok(LighterOrderMinimums {
        min_base_amount: normalize_non_negative_number(base)?,
        min_quote_amount: normalize_non_negative_number(quote)?,
    })
}

// ts: resolveLighterCancelTargets
pub fn resolve_lighter_cancel_targets(
    orders: &[safe_cex_types::Order],
    markets_by_symbol: &indexmap::IndexMap<String, safe_cex_types::Market>,
) -> Vec<LighterCancelTarget> {
    orders
        .iter()
        .filter_map(|order| {
            let market = markets_by_symbol.get(&order.symbol)?;
            let market_id = safe_cex_num::parse_int(&market.id, Some(10));
            market_id.is_finite().then(|| LighterCancelTarget {
                order_id: order.id.clone(),
                market_id,
            })
        })
        .collect()
}

// ts: generateLighterClientId.accountIndex
#[derive(Debug, Clone, Copy)]
pub enum LighterClientIdAccountIndex<'a> {
    String(&'a str),
    BigInt(&'a num_bigint::BigInt),
}

// ts: generateLighterClientId
// Clock/random values are supplied by the runtime at this call boundary, in the
// source's Date.now then Math.random evaluation order.
// ts-bug: generateLighterClientId slices JS's decimal spelling, so a tiny random
// value spelled with an exponent produces a shorter suffix containing '-'.
pub fn generate_lighter_client_id(
    account_index: LighterClientIdAccountIndex<'_>,
    now_ms: f64,
    random: f64,
) -> String {
    let account_index = match account_index {
        LighterClientIdAccountIndex::String(value) => value.to_owned(),
        LighterClientIdAccountIndex::BigInt(value) => value.to_string(),
    };
    let random = js_number_string(random);
    // ECMAScript numeric spelling is ASCII, so chars and UTF-16 indices coincide.
    let suffix: String = random.chars().skip(2).take(6).collect();
    format!(
        "lighter-{account_index}-{}-{suffix}",
        js_number_string(now_ms)
    )
}

// ts: bigIntToString
pub fn big_int_to_string(value: &num_bigint::BigInt) -> String {
    value.to_string()
}

// ts: readNumber
pub fn read_number(
    source: Option<&serde_json::Map<String, serde_json::Value>>,
    keys: &[&str],
    allow_zero: Option<bool>,
) -> Result<Option<f64>, safe_cex_num::JsCoercionError> {
    let Some(source) = source else {
        return Ok(None);
    };
    for key in keys {
        let value = safe_cex_num::js_number(source.get(*key))?;
        let is_valid = if allow_zero.unwrap_or(false) {
            value >= 0.0
        } else {
            value > 0.0
        };
        if value.is_finite() && is_valid {
            return Ok(Some(value));
        }
    }
    Ok(None)
}

// ts: decimalsFromScale
pub fn decimals_from_scale(scale: Option<f64>) -> Option<f64> {
    let scale = scale?;
    if !scale.is_finite() || scale <= 0.0 {
        return None;
    }
    // Shared primitive pins the measured Node darwin-arm64 V8 profile, including
    // contraction-sensitive power neighborhoods; platform f64::log10 differs.
    let log10_scale = safe_cex_num::math_log10(scale);
    if !log10_scale.is_finite() || log10_scale.trunc() != log10_scale {
        return None;
    }
    Some(log10_scale)
}

// ts: getLighterAmountDecimals
pub fn get_lighter_amount_decimals(
    market: &safe_cex_types::Market,
) -> Result<f64, safe_cex_num::JsCoercionError> {
    let info = market.info.as_ref().and_then(serde_json::Value::as_object);
    let amount_decimals = read_number(
        info,
        &[
            "amountDecimals",
            "quantityDecimals",
            "baseDecimals",
            "sizeDecimals",
            "sizePrecision",
        ],
        Some(true),
    )?;
    if let Some(amount_decimals) = amount_decimals {
        return Ok(clamp_decimals(amount_decimals));
    }
    let amount_scale = read_number(
        info,
        &["amountScale", "quantityScale", "baseScale", "sizeScale"],
        None,
    )?;
    if let Some(amount_decimals) = decimals_from_scale(amount_scale) {
        return Ok(clamp_decimals(amount_decimals));
    }
    Ok(clamp_decimals(get_precision(market.precision.amount)))
}

// ts: getLighterPriceDecimals
pub fn get_lighter_price_decimals(
    market: &safe_cex_types::Market,
) -> Result<f64, safe_cex_num::JsCoercionError> {
    let info = market.info.as_ref().and_then(serde_json::Value::as_object);
    let price_decimals = read_number(
        info,
        &[
            "priceDecimals",
            "quoteDecimals",
            "tickDecimals",
            "pricePrecision",
        ],
        Some(true),
    )?;
    if let Some(price_decimals) = price_decimals {
        return Ok(clamp_decimals(price_decimals));
    }
    let price_scale = read_number(info, &["priceScale", "quoteScale", "tickScale"], None)?;
    if let Some(price_decimals) = decimals_from_scale(price_scale) {
        return Ok(clamp_decimals(price_decimals));
    }
    Ok(clamp_decimals(get_precision(market.precision.price)))
}

// ts: stringToBigInt
pub fn string_to_big_int(value: &str) -> Result<num_bigint::BigInt, safe_cex_num::JsBigIntError> {
    safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::String(value.to_owned()))
}

// ts: toLighterScaledInt
pub fn to_lighter_scaled_int(
    value: f64,
    decimals: f64,
) -> Result<num_bigint::BigInt, safe_cex_num::JsBigIntError> {
    if !value.is_finite() {
        let message = format!(
            "Cannot scale non-finite value: {}",
            safe_cex_num::js_number_string(value)
        );
        return Err(safe_cex_num::JsBigIntError {
            kind: safe_cex_num::JsBigIntErrorKind::HostError("Error"),
            message_utf16: message.encode_utf16().collect(),
            message,
        });
    }
    let safe_decimals = clamp_decimals(decimals);
    let scale = safe_cex_num::math_pow(10.0, safe_decimals);
    if !scale.is_finite() {
        let message = format!(
            "Cannot compute scale for decimals: {}",
            safe_cex_num::js_number_string(safe_decimals)
        );
        return Err(safe_cex_num::JsBigIntError {
            kind: safe_cex_num::JsBigIntErrorKind::HostError("Error"),
            message_utf16: message.encode_utf16().collect(),
            message,
        });
    }
    safe_cex_num::js_bigint_primitive(&safe_cex_num::JsPrimitive::Number(
        safe_cex_num::math_round(value * scale),
    ))
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterCancelTarget {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { order_id, market_id });
    }
}

impl safe_cex_num::numeric_ingress::JsNumberOutput for LighterOrderMinimums {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct { min_base_amount, min_quote_amount });
    }
}
