//! Pure branches used by Bitrue REST reads; transport and store assembly remain separate.
use safe_cex_num::{JsCoercionError, js_number, js_string, js_truthy};
use serde_json::{Map, Value};

pub fn is_success_code(code: Option<&Value>) -> bool {
    match code {
        Some(Value::String(code)) => code == "0" || code == "200",
        Some(code @ Value::Number(_)) => {
            let code = js_number(Some(code)).expect("number coercion cannot throw");
            code == 0.0 || code == 200.0
        }
        _ => false,
    }
}

pub fn normalize_timestamp_ms(ts: Option<f64>) -> Option<f64> {
    let ts = ts.filter(|ts| ts.is_finite())?;
    Some(if ts > 1_000_000_000_000.0 {
        ts.floor()
    } else {
        (ts * 1000.0).floor()
    })
}

pub fn normalize_trade_symbol(raw_symbol: Option<&Value>) -> Result<String, JsCoercionError> {
    let value = if js_truthy(raw_symbol) {
        js_string(raw_symbol)?.to_uppercase()
    } else {
        String::new()
    };
    Ok(if value.contains('_') {
        crate::utils::reverse_ws_symbol(&value)
    } else {
        value
    })
}

pub fn pick_trade_order_id(entry: &Map<String, Value>) -> Result<Option<String>, JsCoercionError> {
    let side = if js_truthy(entry.get("side")) {
        js_string(entry.get("side"))?.to_uppercase()
    } else {
        String::new()
    };
    let preferred = entry.get(if side == "BUY" { "bidId" } else { "askId" });
    let fallback = [
        preferred,
        entry.get("bidId"),
        entry.get("askId"),
        entry.get("tradeId"),
    ]
    .into_iter()
    .flatten()
    .find(|value| !value.is_null());
    fallback.map(|value| js_string(Some(value))).transpose()
}

/// Uses the shared production Store view for the source market lookup.
pub fn resolve_trade_qty(
    entry: &Map<String, Value>,
    store: &dyn safe_cex_store::Store,
    symbol: &str,
    parsed_price: f64,
) -> Result<f64, JsCoercionError> {
    let zero = Value::from(0);
    let raw_qty = js_number(Some(
        entry.get("qty").filter(|v| !v.is_null()).unwrap_or(&zero),
    ))?;
    let raw_amount = js_number(Some(
        entry
            .get("amount")
            .filter(|v| !v.is_null())
            .unwrap_or(&zero),
    ))?;
    let markets = store.markets_by_symbol();
    let multiplier = markets
        .get(symbol)
        .and_then(|market| market.info.as_ref())
        .and_then(|info| info.get("multiplier"));
    let multiplier =
        safe_cex_num::js_retained_number(multiplier.filter(|value| value.is_truthy()).or(Some(
            &safe_cex_types::retained_json::RetainedJsonInput::Float(1.0),
        )))?;
    let qty_from_amount = if parsed_price > 0.0 && raw_amount > 0.0 {
        raw_amount / parsed_price
    } else {
        0.0
    };
    if raw_qty <= 0.0 {
        return Ok(if qty_from_amount > 0.0 {
            qty_from_amount
        } else {
            0.0
        });
    }
    if qty_from_amount <= 0.0 {
        return Ok(raw_qty);
    }
    let epsilon = 1e-8;
    if (raw_qty - qty_from_amount).abs() <= epsilon * safe_cex_num::math_max(1.0, qty_from_amount) {
        return Ok(raw_qty);
    }
    let scaled_qty = raw_qty * multiplier;
    if (scaled_qty - qty_from_amount).abs()
        <= epsilon * safe_cex_num::math_max(1.0, qty_from_amount)
    {
        return Ok(scaled_qty);
    }
    Ok(raw_qty)
}

pub fn parse_max_leverage_from_error_message(message: Option<&str>) -> Option<f64> {
    let message = message?;
    // The source's non-Unicode /i regexp folds ASCII letters here; its \s
    // class is the shared ECMAScript trim set, not Rust's Unicode whitespace.
    let lower = message.to_ascii_lowercase();
    let prefix = "maximum leverage does not exceed";
    for (at, _) in lower.match_indices(prefix) {
        let tail = &message[at + prefix.len()..];
        let digits = safe_cex_num::js_trim_start(tail);
        if tail.len() == digits.len() {
            continue;
        }
        let bytes = digits.as_bytes();
        let mut end = bytes.iter().take_while(|b| b.is_ascii_digit()).count();
        if end == 0 {
            continue;
        }
        if bytes.get(end) == Some(&b'.') {
            let fractional = bytes[end + 1..]
                .iter()
                .take_while(|b| b.is_ascii_digit())
                .count();
            if fractional != 0 {
                end += 1 + fractional;
            }
        }
        let parsed = safe_cex_num::number_from_string(&digits[..end]);
        return (parsed.is_finite() && parsed > 0.0).then_some(parsed);
    }
    None
}

pub fn parse_max_leverage_from_bracket_response(
    response: Option<&Value>,
    symbol: Option<&str>,
) -> Result<Option<f64>, JsCoercionError> {
    let raw_data = response.and_then(|response| response.get("data"));
    let entries: Vec<&Value> = match raw_data {
        Some(Value::Array(entries)) => entries.iter().collect(),
        Some(value) if js_truthy(Some(value)) => vec![value],
        _ => vec![],
    };
    let Some(first) = entries.first() else {
        return Ok(None);
    };
    let target = symbol
        .filter(|symbol| !symbol.is_empty())
        .and_then(|symbol| {
            entries
                .iter()
                .copied()
                .find(|entry| entry.get("contractName").and_then(Value::as_str) == Some(symbol))
        })
        .unwrap_or(first);
    let Some(brackets) = target.get("brackets").and_then(Value::as_array) else {
        return Ok(None);
    };
    let mut candidates = Vec::new();
    // Match map-before-filter: every conversion executes, including a later
    // throwing entry after an otherwise valid maximum has already been seen.
    for entry in brackets {
        if entry.is_null() {
            return Err(JsCoercionError {
                name: "TypeError",
                message: "Cannot read properties of null (reading 'initialLeverage')",
            });
        }
        let value = js_number(entry.get("initialLeverage"))?;
        if value.is_finite() && value > 0.0 {
            candidates.push(value);
        }
    }
    // CHOSEN: avoid the engine-dependent argument-stack limit of source
    // Math.max(...candidates); the manifest and live 200k-entry witness mark it.
    Ok(candidates.into_iter().reduce(safe_cex_num::math_max))
}

pub fn set_market_max_leverage(
    store: &dyn safe_cex_store::Store,
    symbol: &str,
    max_leverage: f64,
) -> Result<(), safe_cex_exchange::runtime::ExchangeError> {
    if !max_leverage.is_finite() || max_leverage <= 0.0 {
        return Ok(());
    }
    let selected = store.markets_by_symbol().get(symbol).cloned();
    let Some(market) = selected else {
        if matches!(
            safe_cex_num::js_table_lookup::<&str, ()>(&[], symbol),
            safe_cex_num::TableHit::Inherited(_)
        ) {
            let mut error = safe_cex_exchange::runtime::ExchangeError::new(
                "Cannot read properties of undefined (reading 'leverage')",
            );
            error.name = "TypeError".into();
            return Err(error);
        }
        return Ok(());
    };
    let current_max = market.limits.leverage.max;
    if current_max.is_finite() && (current_max - max_leverage).abs() < 1e-8 {
        return Ok(());
    }
    let mut limits = market.limits;
    limits.leverage.max = max_leverage;
    let mut changes = safe_cex_store::model_patch::ModelPatch::default();
    changes.set_native("limits", limits);
    let update = store.update_market(&market.id, &changes);
    update.map_err(safe_cex_exchange::runtime::ExchangeError::from)
}
