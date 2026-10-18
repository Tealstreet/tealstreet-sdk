//! Position mappers keep Classic v2 and Unified v3 field precedence separate.
use crate::{normalization::*, numeric::*};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::{Position, model_utils::create_position_id};
use serde_json::Value;

// ts: BitgetExchange.mapUtaPosition
pub fn map_uta_position(
    p: Option<&Value>,
    mut normalize_symbol: impl FnMut(Option<&Value>) -> Result<String, ExchangeError>,
    positions: &[Position],
    mut now: impl FnMut() -> f64,
) -> Result<Position, ExchangeError> {
    let p = p.filter(|p| !p.is_null()).ok_or_else(|| ExchangeError {
        name: "TypeError".into(),
        message: if p.is_none() {
            "Cannot read properties of undefined (reading 'symbol')"
        } else {
            "Cannot read properties of null (reading 'symbol')"
        }
        .into(),
        metadata: Box::default(),
    })?;
    let coercion = |e: safe_cex_num::JsCoercionError| ExchangeError {
        name: e.name.into(),
        message: e.message.into(),
        metadata: Box::default(),
    };
    let finite = |keys: &[&str], fallback: f64| {
        let values: Vec<_> = keys
            .iter()
            .map(|key| match p.get(key) {
                Some(Value::String(s)) => FiniteNumberInput::String(s),
                Some(Value::Number(n)) => n
                    .as_f64()
                    .map(FiniteNumberInput::Number)
                    .unwrap_or(FiniteNumberInput::Other),
                _ => FiniteNumberInput::Other,
            })
            .collect();
        first_finite_number(&values, fallback)
    };
    let symbol = normalize_symbol(p.get("symbol"))?;
    let position_mode = normalize_bitget_position_mode(Some(p)).map_err(coercion)?;
    let side = normalize_bitget_position_side(Some(p)).map_err(coercion)?;
    let margin_mode = normalize_bitget_margin_type(p.get("marginMode")).map_err(coercion)?;
    let contracts = finite(&["total", "size"], 0.0);
    let mark_price = finite(&["markPrice", "marketPrice"], 0.0);
    let entry_price = finite(&["avgPrice", "openPriceAvg", "openAvgPrice", "avgPx"], 0.0);
    let id = create_position_id(&symbol, Some(side), Some(position_mode));
    let margin_size = finite(&["positionBalance", "marginSize"], f64::NAN);
    let raw_liquidation_price = p
        .get("liquidationPrice")
        .filter(|v| !v.is_null())
        .or_else(|| p.get("liqPrice").filter(|v| !v.is_null()))
        .or_else(|| p.get("liqPx").filter(|v| !v.is_null()));
    Ok(Position {
        id: id.clone(),
        symbol,
        side: side.into(),
        entry_price,
        notional: safe_cex_num::multiply(
            contracts,
            if mark_price == 0.0 {
                entry_price
            } else {
                mark_price
            },
        ),
        leverage: finite(&["leverage", "lever"], 1.0),
        unrealized_pnl: finite(&["unrealisedPnl", "unrealizedPL", "upl"], 0.0),
        // Unified v3 uses funding and separate open/close fees. Classic v2's
        // achievedProfits + totalFee formula must not be folded into this one.
        realized_pnl: (Some(
            finite(&["curRealisedPnl"], 0.0)
                + finite(&["totalFunding"], 0.0)
                + finite(&["openFeeTotal"], 0.0)
                + finite(&["closeFeeTotal"], 0.0)
                - finite(&["deductedFee"], 0.0),
        ))
        .into(),
        contracts,
        liquidation_price: resolve_liquidation_price(&id, raw_liquidation_price, positions)
            .map_err(coercion)?,
        margin_size: if margin_size > 0.0 {
            (Some(margin_size)).into()
        } else {
            (None).into()
        },
        position_mode: (Some(position_mode)).into(),
        margin_type: (Some(margin_mode)).into(),
        info: (Some(p.clone().into())).into(),
        last_update: (Some(now())).into(),
        extra: (None).into(),
        option: (None).into(),
    })
}

// ts: BitgetExchange.mapPosition
pub fn map_position(
    p: Option<&Value>,
    mut normalize_symbol: impl FnMut(Option<&Value>) -> Result<String, ExchangeError>,
    positions: &[Position],
    mut now: impl FnMut() -> f64,
) -> Result<Position, ExchangeError> {
    let p = match p {
        Some(p @ (Value::Object(_) | Value::Array(_))) => p,
        _ => {
            let symbol = String::new();
            let side = safe_cex_types::PositionSide::Long;
            let mode = safe_cex_types::PositionMode::OneWay;
            return Ok(Position {
                id: create_position_id(&symbol, Some(side), Some(mode)),
                symbol,
                side: side.into(),
                entry_price: 0.0,
                notional: 0.0,
                leverage: 1.0,
                unrealized_pnl: 0.0,
                realized_pnl: (Some(0.0)).into(),
                contracts: 0.0,
                liquidation_price: 0.0,
                position_mode: (Some(mode)).into(),
                margin_type: (Some(safe_cex_types::MarginType::Cross)).into(),
                info: p.cloned().map(Into::into).into(),
                last_update: (Some(now())).into(),
                margin_size: (None).into(),
                extra: (None).into(),
                option: (None).into(),
            });
        }
    };
    let coercion = |e: safe_cex_num::JsCoercionError| ExchangeError {
        name: e.name.into(),
        message: e.message.into(),
        metadata: Box::default(),
    };
    let finite = |keys: &[&str], fallback: f64| {
        let values: Vec<_> = keys
            .iter()
            .map(|key| match p.get(key) {
                Some(Value::String(s)) => FiniteNumberInput::String(s),
                Some(Value::Number(n)) => n
                    .as_f64()
                    .map(FiniteNumberInput::Number)
                    .unwrap_or(FiniteNumberInput::Other),
                _ => FiniteNumberInput::Other,
            })
            .collect();
        first_finite_number(&values, fallback)
    };
    let contracts = finite(&["total", "size", "pos", "openTotalPos", "available"], 0.0);
    let entry_price = finite(
        &[
            "openPriceAvg",
            "openAvgPrice",
            "avgPrice",
            "avgPx",
            "averageOpenPrice",
        ],
        0.0,
    );
    let mark_price = finite(&["markPrice", "marketPrice"], entry_price);
    let symbol = normalize_symbol(
        p.get("instId")
            .filter(|v| !v.is_null())
            .or_else(|| p.get("symbol")),
    )?;
    let position_mode = normalize_bitget_position_mode(Some(p)).map_err(coercion)?;
    let margin_mode = normalize_bitget_margin_type(p.get("marginMode")).map_err(coercion)?;
    let side = normalize_bitget_position_side(Some(p)).map_err(coercion)?;
    let margin_size = finite(&["marginSize", "margin", "positionBalance"], f64::NAN);
    let id = create_position_id(&symbol, Some(side), Some(position_mode));
    let raw_liquidation_price = p
        .get("liquidationPrice")
        .filter(|v| !v.is_null())
        .or_else(|| p.get("liqPrice").filter(|v| !v.is_null()))
        .or_else(|| p.get("liqPx").filter(|v| !v.is_null()));
    Ok(Position {
        id: id.clone(),
        symbol,
        side: side.into(),
        entry_price,
        notional: safe_cex_num::multiply(
            contracts,
            if mark_price == 0.0 {
                entry_price
            } else {
                mark_price
            },
        ),
        leverage: finite(&["leverage", "lever"], 1.0),
        unrealized_pnl: finite(&["unrealizedPL", "unrealisedPnl", "upl"], 0.0),
        // Classic v2 realized PnL excludes Unified funding/open/close fields.
        realized_pnl: (Some(
            finite(&["achievedProfits"], 0.0) + finite(&["totalFee"], 0.0)
                - finite(&["deductedFee"], 0.0),
        ))
        .into(),
        contracts,
        liquidation_price: resolve_liquidation_price(&id, raw_liquidation_price, positions)
            .map_err(coercion)?,
        margin_size: if margin_size > 0.0 {
            (Some(margin_size)).into()
        } else {
            (None).into()
        },
        position_mode: (Some(position_mode)).into(),
        margin_type: (Some(margin_mode)).into(),
        info: (Some(p.clone().into())).into(),
        last_update: (Some(now())).into(),
        extra: (None).into(),
        option: (None).into(),
    })
}
