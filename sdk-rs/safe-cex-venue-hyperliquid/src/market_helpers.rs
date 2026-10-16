//! Market scalar and fast-context leaves, with source-bound external primitives.
use crate::{
    api_errors::{ApiErrorClass, ApiErrorHost},
    spot_json::JsString,
};
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PreciseComparison {
    Equal,
    Greater,
    Less,
}
pub trait MarketScalarHost: ApiErrorHost {
    fn number_value(&self, value: &Self::Value) -> Option<f64>;
    /// Actual utils/get-key v including presence/prototype and snake/camel fallback.
    fn value_by_key(&mut self, value: &Self::Value, key: &str) -> Result<Self::Value, Self::Error>;
    /// JavaScript parseFloat on the supplied primitive UTF-16 string.
    fn parse_float(&mut self, text: &[u16]) -> Result<f64, Self::Error>;
    fn lowercase(&mut self, text: &[u16]) -> Result<JsString, Self::Error>;
    fn number_to_string(&mut self, number: f64) -> Result<Option<JsString>, Self::Error>;
    fn precise_compare(
        &mut self,
        kind: PreciseComparison,
        left: &[u16],
        right: &str,
    ) -> Result<bool, Self::Error>;
    fn parse_to_int(&mut self, number: f64) -> Result<f64, Self::Error>;
}
fn finite<H: MarketScalarHost>(host: &mut H, value: &H::Value) -> Result<f64, H::Error> {
    let parsed = if let Some(number) = host.number_value(value) {
        number
    } else {
        let string_value = if host.is_nullish(value) {
            host.literal("")
        } else {
            value.clone()
        };
        let text = host.to_string(&string_value)?;
        host.parse_float(&text.0)?
    };
    Ok(if parsed.is_finite() { parsed } else { 0.0 })
}
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct VolumeFields {
    pub volume: f64,
    pub quote_volume: f64,
}
pub fn parse_hyperliquid_volume_fields<H: MarketScalarHost>(
    host: &mut H,
    ticker: &H::Value,
    price: f64,
) -> Result<VolumeFields, H::Error> {
    let raw = host.value_by_key(ticker, "dayNtlVlm")?;
    let quote_volume = finite(host, &raw)?;
    let raw = host.value_by_key(ticker, "dayBaseVlm")?;
    let day_base_volume = finite(host, &raw)?;
    let volume = if day_base_volume > 0.0 {
        day_base_volume
    } else if price > 0.0 {
        quote_volume / price
    } else {
        0.0
    };
    Ok(VolumeFields {
        volume,
        quote_volume,
    })
}
pub fn parse_hyperliquid_open_interest_notional<H: MarketScalarHost>(
    host: &mut H,
    ticker: &H::Value,
    price: f64,
) -> Result<f64, H::Error> {
    let raw = host.value_by_key(ticker, "openInterest")?;
    let base = finite(host, &raw)?;
    Ok(if price > 0.0 { base * price } else { 0.0 })
}
pub fn symbol_to_coin<H: MarketScalarHost>(
    host: &mut H,
    symbol: &JsString,
) -> Result<JsString, H::Error> {
    let Some(pipe) = symbol.0.iter().position(|unit| *unit == b'|' as u16) else {
        return Ok(symbol.clone());
    };
    let mut end = symbol.0.len();
    const SUFFIX: &[u8] = b"-PERP";
    if end >= 5
        && symbol.0[end - 5..]
            .iter()
            .zip(SUFFIX)
            .all(|(unit, letter)| {
                *unit == *letter as u16
                    || (*letter != b'-' && *unit == letter.to_ascii_lowercase() as u16)
            })
    {
        end -= 5;
    }
    let dex = host.lowercase(&symbol.0[..pipe])?;
    let asset = &symbol.0[pipe + 1..end];
    let asset_end = asset
        .iter()
        .position(|unit| *unit == b'|' as u16)
        .unwrap_or(asset.len());
    let mut result = dex.0;
    result.push(b':' as u16);
    result.extend(&asset[..asset_end]);
    Ok(JsString(result))
}
fn js_min(left: f64, right: f64) -> f64 {
    if left.is_nan() || right.is_nan() {
        return f64::NAN;
    }
    if left == 0.0 && right == 0.0 {
        return if left.is_sign_negative() || right.is_sign_negative() {
            -0.0
        } else {
            0.0
        };
    }
    if left < right { left } else { right }
}
pub fn calculate_price_precision<H: MarketScalarHost>(
    host: &mut H,
    price: f64,
    amount_precision: f64,
    max_decimals: f64,
) -> Result<f64, H::Error> {
    let Some(text) = host.number_to_string(price)? else {
        return Ok(0.0);
    };
    let parts: Vec<&[u16]> = text.0.split(|unit| *unit == b'.' as u16).collect();
    let precision = if host.precise_compare(PreciseComparison::Equal, &text.0, "0")? {
        js_min(max_decimals - amount_precision, 5.0)
    } else if host.precise_compare(PreciseComparison::Greater, &text.0, "0")?
        && host.precise_compare(PreciseComparison::Less, &text.0, "1")?
    {
        let decimal = parts.get(1).copied().unwrap_or(&[]);
        if decimal.is_empty() {
            return Ok(0.0);
        }
        let leading = decimal
            .iter()
            .take_while(|unit| **unit == b'0' as u16)
            .count();
        js_min(max_decimals - amount_precision, leading as f64 + 5.0)
    } else {
        let integer = parts.first().copied().unwrap_or(&[]);
        if integer.is_empty() {
            return Ok(0.0);
        }
        let length = integer.len() as f64;
        js_min(
            max_decimals - amount_precision,
            5.0_f64.max(length) - length,
        )
    };
    host.parse_to_int(precision)
}
pub trait FastAssetCtxHost: ApiErrorHost {
    fn atob(&mut self, text: &[u16]) -> Result<JsString, Self::Error>;
    fn inflate_raw(&mut self, compressed: &[u8]) -> Result<Vec<u8>, Self::Error>;
    /// new TextDecoder().decode(bytes), default nonfatal UTF-8/BOM behavior.
    fn decode_text(&mut self, bytes: &[u8]) -> Result<JsString, Self::Error>;
}
fn invalid_fast<H: ApiErrorHost>(host: &mut H, message: &str) -> Result<H::Value, H::Error> {
    let text = host.literal(message);
    let error = host.new_error(ApiErrorClass::Intrinsic, &text)?;
    Err(host.exception_from_value(error))
}
pub fn decode_hyperliquid_fast_asset_ctxs<H: FastAssetCtxHost>(
    host: &mut H,
    payload: &H::Value,
) -> Result<H::Value, H::Error> {
    let Some(text) = host.string_units(payload).map(<[u16]>::to_vec) else {
        return invalid_fast(host, "Invalid Hyperliquid fastAssetCtxs payload");
    };
    if text.is_empty() {
        return invalid_fast(host, "Invalid Hyperliquid fastAssetCtxs payload");
    }
    let decoded = host.atob(&text)?;
    // String iteration visits code points; charCodeAt(0) selects the first UTF-16
    // unit of each point, then Uint8Array truncates it. Standard atob is Latin-1.
    let mut bytes = Vec::new();
    let mut index = 0;
    while index < decoded.0.len() {
        let unit = decoded.0[index];
        bytes.push(unit as u8);
        index += 1;
        if (0xd800..=0xdbff).contains(&unit)
            && decoded
                .0
                .get(index)
                .is_some_and(|low| (0xdc00..=0xdfff).contains(low))
        {
            index += 1;
        }
    }
    let inflated = host.inflate_raw(&bytes)?;
    let text = host.decode_text(&inflated)?;
    let data = host.parse_json(&text.0)?;
    if !host.truthy(&data) || host.is_array(&data)? || !host.is_object(&data) {
        return invalid_fast(host, "Invalid Hyperliquid fastAssetCtxs data");
    }
    Ok(data)
}

pub trait MetaTupleHost: ApiErrorHost {
    /// Destructure two values using the source iterator protocol/IteratorClose.
    fn first_two(&mut self, data: &Self::Value) -> Result<[Self::Value; 2], Self::Error>;
    fn type_of(&self, value: &Self::Value) -> &'static str;
    /// Template ToString (unlike explicit String, Symbols throw).
    fn implicit_string(&mut self, value: &Self::Value) -> Result<JsString, Self::Error>;
    /// Invoke the supplied logger with LogSeverity.Warning; preserve exceptions.
    fn log_warning(&mut self, message: &JsString) -> Result<(), Self::Error>;
}
pub fn parse_meta_and_asset_ctxs_response<H: MetaTupleHost>(
    host: &mut H,
    data: &H::Value,
    source_label: &JsString,
    context_label: &JsString,
) -> Result<Option<[H::Value; 2]>, H::Error> {
    if !host.is_array(data)? {
        let mut message: Vec<u16> = "Invalid ".encode_utf16().collect();
        message.extend(&context_label.0);
        message.extend(" response from ".encode_utf16());
        message.extend(&source_label.0);
        message.extend(": expected tuple array, got ".encode_utf16());
        message.extend(host.type_of(data).encode_utf16());
        host.log_warning(&JsString(message))?;
        return Ok(None);
    }
    let [meta, assets] = host.first_two(data)?;
    let universe = if host.is_nullish(&meta) {
        host.undefined()
    } else {
        host.get(&meta, "universe")?
    };
    let has_universe = host.is_array(&universe)?;
    let has_assets = host.is_array(&assets)?;
    if !has_universe || !has_assets {
        let length = host.get(data, "length")?;
        let length = host.implicit_string(&length)?;
        let mut message: Vec<u16> = "Invalid ".encode_utf16().collect();
        message.extend(&context_label.0);
        message.extend(" shape from ".encode_utf16());
        message.extend(&source_label.0);
        message.extend(": tupleLength=".encode_utf16());
        message.extend(length.0);
        message.extend(
            format!(", hasUniverse={has_universe}, hasAssetCtxs={has_assets}").encode_utf16(),
        );
        host.log_warning(&JsString(message))?;
        return Ok(None);
    }
    Ok(Some([meta, assets]))
}
