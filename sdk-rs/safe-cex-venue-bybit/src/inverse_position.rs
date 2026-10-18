use crate::exchange_state::parse_float;
use safe_cex_num::{js_number_string, js_truthy, math_round};
use safe_cex_types::{MarginType, Position, PositionExtra, PositionMode, PositionSide};
use serde_json::Value;

use crate::json_field::field;
fn numeric_or_zero(value: Option<&Value>) -> Result<f64, &'static str> {
    parse_float(
        value
            .filter(|v| js_truthy(Some(v)))
            .or(Some(&serde_json::json!(0))),
    )
}

pub fn map_position(
    raw: &Value,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<MarginType>,
    now: impl FnMut() -> f64,
    log: impl FnMut(String),
) -> Result<Position, String> {
    match map_position_deferred(raw, store, margin_type, now, log)? {
        PositionMapping::Mapped(position) => Ok(*position),
        PositionMapping::Refused(message) => Err(message),
    }
}

// A source row can finish without fitting the closed Position model. Keep that
// outcome distinct from a source throw so batch callers continue later work.
pub(crate) enum PositionMapping {
    Mapped(Box<Position>),
    Refused(String),
}

pub(crate) fn map_position_deferred(
    raw: &Value,
    store: &dyn safe_cex_store::Store,
    margin_type: Option<MarginType>,
    mut now: impl FnMut() -> f64,
    mut log: impl FnMut(String),
) -> Result<PositionMapping, String> {
    if raw.is_null() {
        return Err("Cannot read properties of null (reading 'size')".into());
    }
    let entry = match field(raw, "avgPrice")?.filter(|v| js_truthy(Some(v))) {
        Some(value) => Some(value),
        None => field(raw, "entryPrice")?,
    };
    let entry_price = numeric_or_zero(entry)?;
    let contracts_native = parse_float(
        raw.get("size")
            .filter(|v| !v.is_null())
            .or(Some(&serde_json::json!(0))),
    )?;
    let index = raw.get("positionIdx");
    let side = match index.and_then(Value::as_f64) {
        Some(1.) => Ok(PositionSide::Long),
        Some(2.) => Ok(PositionSide::Short),
        _ => {
            // Property-key coercion is lazy: numeric positionIdx bypasses it.
            let key = safe_cex_num::js_string(raw.get("side"))
                .map_err(|error| error.message.to_owned())?;
            let entries: Vec<_> = crate::types::POSITION_SIDE
                .as_object()
                .into_iter()
                .flat_map(|table| table.iter())
                .map(|(key, value)| (key.as_str(), value))
                .collect();
            match safe_cex_num::js_table_lookup(&entries, &key) {
                safe_cex_num::TableHit::Mapped(value) => {
                    Ok(serde_json::from_value((*value).clone())
                        .map_err(|error| error.to_string())?)
                }
                safe_cex_num::TableHit::Missing => Ok(PositionSide::Long),
                // ts-bug: an inherited function/object suppresses the source || fallback.
                // Preserve the distinction until the typed Position boundary refuses it.
                safe_cex_num::TableHit::Inherited(_) => Err(format!(
                    "Bybit typed Position cannot represent inherited side key {key}"
                )),
            }
        }
    };
    let pnl = numeric_or_zero(field(raw, "unrealisedPnl")?)?;
    // The source coerces this property key here, after side/PnL evaluation.
    // Keep non-string symbols alive until the typed output boundary too.
    let symbol_key =
        safe_cex_num::js_string(raw.get("symbol")).map_err(|error| error.message.to_owned())?;
    let symbol = symbol_key.as_str();
    let inverse = store
        .markets_by_symbol()
        .get(symbol)
        .and_then(|m| m.is_inverse)
        .unwrap_or(false);
    let mut notional = numeric_or_zero(field(raw, "positionValue")?)?;
    let mut contracts = contracts_native;
    if inverse {
        let ticker = store.tickers_by_symbol().get(symbol).cloned();
        let price = ticker
            .as_ref()
            .map(|t| t.mark)
            .filter(|n| *n != 0. && !n.is_nan())
            .or_else(|| {
                ticker
                    .as_ref()
                    .map(|t| t.last)
                    .filter(|n| *n != 0. && !n.is_nan())
            })
            .unwrap_or(entry_price);
        if price > 0. {
            contracts = math_round(contracts_native / price * 1e8) / 1e8;
            if contracts_native > 0. {
                log(format!(
                    "BybitInverse position: {symbol} - USD contracts: {}, BTC price: {}, BTC amount: {}",
                    js_number_string(contracts_native),
                    js_number_string(price),
                    js_number_string(contracts)
                ));
            }
        }
        notional = contracts_native;
    }
    let mode = index.map(|v| {
        if v.as_f64() == Some(0.) {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        }
    });
    let margin = numeric_or_zero(field(raw, "positionIM")?)?;
    // Evaluate every remaining source expression, including its clock side
    // effect, before applying the closed-output refusal.
    let leverage = parse_float(raw.get("leverage"))?;
    let realized_pnl = numeric_or_zero(field(raw, "curRealisedPnl")?)?;
    let liquidation_price = numeric_or_zero(field(raw, "liqPrice")?)?;
    let last_update = now();
    let Some(symbol) = raw.get("symbol").and_then(Value::as_str) else {
        return Ok(PositionMapping::Refused(
            "Bybit typed Position requires string symbol".into(),
        ));
    };
    let side = match side {
        Ok(side) => side,
        Err(message) => return Ok(PositionMapping::Refused(message)),
    };
    Ok(PositionMapping::Mapped(Box::new(Position {
        id: safe_cex_types::model_utils::create_position_id(symbol, Some(side), mode),
        symbol: symbol.into(),
        side: side.into(),
        entry_price,
        notional,
        leverage,
        margin_type: margin_type.into(),
        position_mode: (mode).into(),
        unrealized_pnl: pnl,
        realized_pnl: (Some(realized_pnl)).into(),
        contracts,
        liquidation_price,
        margin_size: ((margin > 0.).then_some(margin)).into(),
        info: (Some(raw.clone().into())).into(),
        extra: (Some(PositionExtra {
            contracts_native: Some(contracts_native),
            unrealized_pnl_native: Some(pnl),
        }))
        .into(),
        last_update: (Some(last_update)).into(),
        option: (None).into(),
    })))
}

#[cfg(test)]
mod tests;
