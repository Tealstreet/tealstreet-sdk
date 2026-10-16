//! Account and position field normalization shared by the two API generations.
use safe_cex_num::{JsCoercionError, js_string};
use safe_cex_types::{MarginType, PositionMode, PositionSide};
use serde_json::Value;

// ts: BitgetExchange.normalizeBitgetPositionMode
pub fn normalize_bitget_position_mode(
    value: Option<&Value>,
) -> Result<PositionMode, JsCoercionError> {
    let value = value.filter(|v| !v.is_null()).ok_or(JsCoercionError {
        name: "TypeError",
        message: if value.is_none() {
            "Cannot read properties of undefined (reading 'posMode')"
        } else {
            "Cannot read properties of null (reading 'posMode')"
        },
    })?;
    let empty = Value::String(String::new());
    let raw = value
        .get("posMode")
        .filter(|v| !v.is_null())
        .or_else(|| value.get("holdMode").filter(|v| !v.is_null()))
        .unwrap_or(&empty);
    let raw_mode = js_string(Some(raw))?.to_lowercase();
    Ok(if raw_mode == "hedge_mode" || raw_mode == "double_hold" {
        PositionMode::Hedged
    } else {
        PositionMode::OneWay
    })
}

// ts: BitgetExchange.normalizeBitgetMarginType
pub fn normalize_bitget_margin_type(value: Option<&Value>) -> Result<MarginType, JsCoercionError> {
    let empty = Value::String(String::new());
    let raw = value.filter(|v| !v.is_null()).unwrap_or(&empty);
    let raw_margin_type = js_string(Some(raw))?.to_lowercase();
    Ok(
        if raw_margin_type == "crossed" || raw_margin_type == "cross" {
            MarginType::Cross
        } else {
            MarginType::Isolated
        },
    )
}

// ts: BitgetExchange.normalizeBitgetPositionSide
pub fn normalize_bitget_position_side(
    value: Option<&Value>,
) -> Result<PositionSide, JsCoercionError> {
    let value = value.filter(|v| !v.is_null()).ok_or(JsCoercionError {
        name: "TypeError",
        message: if value.is_none() {
            "Cannot read properties of undefined (reading 'holdSide')"
        } else {
            "Cannot read properties of null (reading 'holdSide')"
        },
    })?;
    let empty = Value::String(String::new());
    let raw = value
        .get("holdSide")
        .filter(|v| !v.is_null())
        .or_else(|| value.get("posSide").filter(|v| !v.is_null()))
        .or_else(|| value.get("side").filter(|v| !v.is_null()))
        .unwrap_or(&empty);
    let raw_side = js_string(Some(raw))?.to_lowercase();
    Ok(if raw_side == "short" || raw_side == "sell" {
        PositionSide::Short
    } else {
        PositionSide::Long
    })
}

// ts: isBitgetSuccessCode
pub fn is_bitget_success_code(code: Option<&Value>) -> Result<bool, JsCoercionError> {
    let zero = Value::from(0);
    let code = code.filter(|v| !v.is_null()).unwrap_or(&zero);
    Ok(safe_cex_num::parse_int(&js_string(Some(code))?, Some(10)) == 0.0)
}

// ts: BitgetExchange.resolveLiquidationPrice
pub fn resolve_liquidation_price(
    position_id: &str,
    raw_liquidation_price: Option<&Value>,
    positions: &[safe_cex_types::Position],
) -> Result<f64, JsCoercionError> {
    let empty = Value::String(String::new());
    let raw = raw_liquidation_price
        .filter(|v| !v.is_null())
        .unwrap_or(&empty);
    let parsed = safe_cex_num::parse_float(&js_string(Some(raw))?);
    if parsed.is_finite() {
        return Ok(parsed);
    }
    let stored = positions
        .iter()
        .find(|p| p.id == position_id)
        .map(|p| p.liquidation_price)
        .unwrap_or(0.0);
    // The shared Position field is f64: the source numeric `|| 0` changes
    // NaN and both signed zeros to positive zero, while retaining infinities.
    Ok(if stored == 0.0 || stored.is_nan() {
        0.0
    } else {
        stored
    })
}

// ts: BitgetExchange.normalizeBitgetSymbol
// Receiver profile projection: these are the three source classes, each with
// live/testnet product getters. This is not a deserializable wire union.
#[derive(Clone, Copy)]
pub enum SymbolProfile {
    Usdt,
    Usdc,
    Inverse,
}

// ts: BitgetExchange.normalizeBitgetSymbol
pub fn normalize_bitget_symbol(
    value: Option<&Value>,
    profile: SymbolProfile,
    testnet: bool,
    mut resolve_symbol: impl FnMut(
        &str,
    )
        -> Result<Option<String>, safe_cex_exchange::runtime::ExchangeError>,
) -> Result<String, safe_cex_exchange::runtime::ExchangeError> {
    let empty = Value::String(String::new());
    let raw = value.filter(|v| !v.is_null()).unwrap_or(&empty);
    let mut stripped =
        js_string(Some(raw)).map_err(|e| safe_cex_exchange::runtime::ExchangeError {
            name: e.name.into(),
            message: e.message.into(),
            metadata: Box::default(),
        })?;
    let product = match profile {
        SymbolProfile::Usdt => crate::profile::api_product_type(testnet),
        SymbolProfile::Usdc => crate::profile::usdc_api_product_type(testnet),
        SymbolProfile::Inverse => crate::profile::inverse_api_product_type(testnet),
    };
    // The six source getter values contain only ASCII letters and a hyphen.
    // Their dynamic /i regex therefore has exactly this suffix behavior.
    let product_suffix = format!("_{product}");
    if let Some(start) = stripped.len().checked_sub(product_suffix.len())
        && stripped
            .get(start..)
            .is_some_and(|s| s.eq_ignore_ascii_case(&product_suffix))
    {
        stripped.truncate(start);
    }
    for suffix in ["_UMCBL", "_SUMCBL", "_DMCBL", "_CMCBL"] {
        if let Some(start) = stripped.len().checked_sub(suffix.len())
            && stripped
                .get(start..)
                .is_some_and(|s| s.eq_ignore_ascii_case(suffix))
        {
            stripped.truncate(start);
            break;
        }
    }
    Ok(resolve_symbol(&stripped)?.unwrap_or(stripped))
}
