//! Pure Phemex instance-method bodies with explicit inputs.
use safe_cex_types::PositionSide;

pub fn calculate_price_diff(price: f64, mark_price: f64, side: PositionSide) -> f64 {
    if side == PositionSide::Long {
        safe_cex_num::subtract(mark_price, price)
    } else {
        safe_cex_num::subtract(price, mark_price)
    }
}

/// Shared UTF-8 string boundary; absent input follows the source empty fallback.
pub fn format_symbol_in(symbol: Option<&str>) -> String {
    symbol.unwrap_or("").to_uppercase()
}

/// The first matching market wins even when its info symbol is falsy.
/// Native info is returned without projecting its type or numeric value.
pub fn format_symbol_out(
    markets: Option<&[safe_cex_types::Market]>,
    symbol: Option<&str>,
) -> safe_cex_types::retained_json::RetainedJsonInput {
    let found = markets
        .and_then(|markets| {
            markets
                .iter()
                .find(|market| Some(market.symbol.as_str()) == symbol)
        })
        .and_then(|market| market.info.as_ref())
        .and_then(|info| info.get("symbol"));
    if found.is_some_and(|value| value.is_truthy()) {
        found.expect("truthy value exists").clone()
    } else {
        safe_cex_types::retained_json::RetainedJsonInput::Text(
            symbol.unwrap_or("").encode_utf16().collect(),
        )
    }
}

/// Production language-value entry: keep source UTF-16 units until a typed consumer.
pub fn format_symbol_in_value(
    symbol: safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::Js;
    if symbol.is_nullish() {
        return Ok(Js::str(""));
    }
    // Preserve code units outside scalar Unicode. The shared string builtin's
    // UTF-8 conversion currently substitutes lone surrogates, which changes
    // source-visible symbol lookup and request values.
    let upper = if let Js::String(units) = &symbol {
        let mut result = Vec::new();
        for decoded in char::decode_utf16(units.iter().copied()) {
            match decoded {
                Ok(c) => {
                    for upper in c.to_uppercase() {
                        result.extend_from_slice(upper.encode_utf16(&mut [0; 2]));
                    }
                }
                Err(error) => result.push(error.unpaired_surrogate()),
            }
        }
        Js::utf16(&result)
    } else {
        symbol.call_method("toUpperCase", vec![])?
    };
    Ok(if upper.truthy() {
        upper
    } else if symbol.truthy() {
        symbol
    } else {
        Js::str("")
    })
}
/// Production language-value entry over the live Store facade, retaining raw info.
pub fn format_symbol_out_value(
    markets: &safe_cex_exchange::js_runtime::value::Js,
    symbol: safe_cex_exchange::js_runtime::value::Js,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
{
    use safe_cex_exchange::js_runtime::value::{Js, Thrown};
    for market in markets
        .array_values()
        .ok_or_else(|| Thrown::type_error("markets.find is not a function"))?
    {
        if market.get("symbol")?.strict_equals(&symbol) {
            let info = market.get("info")?;
            let found = if info.is_nullish() {
                Js::Undefined
            } else {
                info.get("symbol")?
            };
            if found.truthy() {
                return Ok(found);
            }
            break;
        }
    }
    Ok(if symbol.truthy() { symbol } else { Js::str("") })
}
