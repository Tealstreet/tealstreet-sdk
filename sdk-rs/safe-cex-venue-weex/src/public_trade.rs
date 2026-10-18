//! Source-corresponding public-trade normalization.
use crate::utils::{
    WeexUtilityError, WeexUtilityInput, normalize_timestamp_to_ms, to_integer_string, to_number,
};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use safe_cex_types::{OrderSide, PublicTrade};

pub fn normalize_public_trade(
    raw: &WeexUtilityInput,
    symbol: &str,
    local_to_utc: impl FnOnce(f64) -> f64,
) -> Result<Option<PublicTrade>, WeexUtilityError> {
    let value = raw.as_value();
    if value.is_null() {
        return Err(WeexUtilityError {
            name: "TypeError",
            message: "Cannot read properties of null (reading 'price')".to_owned(),
        });
    }
    let pick = |keys: &[&str]| raw.pick_retained(keys);
    let price = to_number(pick(&["price", "p"]).as_ref(), None)?;
    let qty = to_number(pick(&["qty", "q"]).as_ref(), None)?.abs();
    let timestamp =
        normalize_timestamp_to_ms(pick(&["time", "T", "createdTime"]).as_ref(), local_to_utc)?;
    if price <= 0.0 || qty <= 0.0 || timestamp == 0.0 {
        return Ok(None);
    }
    let id = match pick(&["id", "t"]) {
        Some(id) => to_integer_string(Some(&id))?,
        None => format!(
            "{}:{}:{}",
            safe_cex_num::js_number_string(timestamp),
            safe_cex_num::js_number_string(price),
            safe_cex_num::js_number_string(qty)
        ),
    };
    let quote_qty = to_number(pick(&["quoteQty", "v"]).as_ref(), None)?;
    let side = if value.get("buyer") == Some(&Input::Bool(false))
        || value.get("m") == Some(&Input::Bool(true))
        || value.get("isBuyerMaker") == Some(&Input::Bool(true))
    {
        OrderSide::Sell
    } else {
        OrderSide::Buy
    };
    Ok(Some(PublicTrade {
        id,
        symbol: symbol.to_owned(),
        price,
        qty,
        quote_qty: Some(quote_qty),
        timestamp,
        side,
    }))
}
