use safe_cex_exchange::js_runtime::value::{Js, Thrown, arg};
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
    precision_from_text(&text)
}

fn precision_from_text(text: &str) -> f64 {
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
pub fn normalize_non_negative_number(value: &Js) -> Result<f64, Thrown> {
    let parsed = if value.is_nullish() || matches!(value, Js::SymbolIterator) {
        f64::NAN
    } else {
        value.parse_float()?
    };
    Ok(if !parsed.is_finite() || parsed < 0.0 {
        0.0
    } else {
        parsed
    })
}

fn nullish_property(value: &Js, key: &str) -> Result<Js, Thrown> {
    if value.is_nullish() {
        Ok(Js::Undefined)
    } else {
        value.get(key)
    }
}

fn minimum(info: &Js, primary: &str, alias: &str) -> Result<f64, Thrown> {
    let value = nullish_property(info, primary)?;
    let value = if value.is_nullish() {
        nullish_property(info, alias)?
    } else {
        value
    };
    normalize_non_negative_number(&value)
}

// ts: getLighterOrderMinimums
pub fn get_lighter_order_minimums(market: &Js) -> Result<LighterOrderMinimums, Thrown> {
    let info = market.get("info")?;
    Ok(LighterOrderMinimums {
        min_base_amount: minimum(&info, "min_base_amount", "minBaseAmount")?,
        min_quote_amount: minimum(&info, "min_quote_amount", "minQuoteAmount")?,
    })
}

// ts: resolveLighterCancelTargets
pub fn resolve_lighter_cancel_targets(orders: &Js, markets_by_symbol: &Js) -> Result<Js, Thrown> {
    let markets = markets_by_symbol.clone();
    orders.call_method(
        "flatMap",
        vec![Js::function("cancel target", move |_, args| {
            let order = arg(&args, 0);
            let market = markets.get_property(&order.get("symbol")?)?;
            if !market.truthy() {
                return Ok(Js::array(vec![]));
            }
            let market_id = safe_cex_num::parse_int(
                &String::from_utf16_lossy(&market.get("id")?.to_js_string()?),
                Some(10),
            );
            if !market_id.is_finite() {
                return Ok(Js::array(vec![]));
            }
            Ok(Js::array(vec![Js::object(vec![
                ("orderId", order.get("id")?),
                ("marketId", Js::Number(market_id)),
            ])]))
        })],
    )
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
    source: &Js,
    keys: &[&str],
    allow_zero: Option<bool>,
) -> Result<Option<f64>, Thrown> {
    if !source.truthy() {
        return Ok(None);
    }
    for key in keys {
        let value = source.get(key)?.to_number()?;
        let valid = if allow_zero.unwrap_or(false) {
            value >= 0.0
        } else {
            value > 0.0
        };
        if value.is_finite() && valid {
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

fn market_decimals(
    market: &Js,
    decimals: &[&str],
    scales: &[&str],
    precision: &str,
) -> Result<f64, Thrown> {
    let info = market.get("info")?;
    if let Some(value) = read_number(&info, decimals, Some(true))? {
        return Ok(clamp_decimals(value));
    }
    if let Some(value) = decimals_from_scale(read_number(&info, scales, None)?) {
        return Ok(clamp_decimals(value));
    }
    let value = market.get("precision")?.get(precision)?;
    let decimals = if value.to_number()?.is_nan() {
        0.0
    } else {
        precision_from_text(&String::from_utf16_lossy(&value.to_js_string()?))
    };
    Ok(clamp_decimals(decimals))
}

// ts: getLighterAmountDecimals
pub fn get_lighter_amount_decimals(market: &Js) -> Result<f64, Thrown> {
    market_decimals(
        market,
        &[
            "amountDecimals",
            "quantityDecimals",
            "baseDecimals",
            "sizeDecimals",
            "sizePrecision",
        ],
        &["amountScale", "quantityScale", "baseScale", "sizeScale"],
        "amount",
    )
}

// ts: getLighterPriceDecimals
pub fn get_lighter_price_decimals(market: &Js) -> Result<f64, Thrown> {
    market_decimals(
        market,
        &[
            "priceDecimals",
            "quoteDecimals",
            "tickDecimals",
            "pricePrecision",
        ],
        &["priceScale", "quoteScale", "tickScale"],
        "price",
    )
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
