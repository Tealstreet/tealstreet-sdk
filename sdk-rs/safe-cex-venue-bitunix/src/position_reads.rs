//! Position response mapping and concurrent order-family aggregation.
use crate::{
    row_extraction::{extract_position_rows, get_first_value},
    value_helpers::parse_finite_number,
};
use safe_cex_num::{JsCoercionError, TableHit, js_string, js_truthy};
use safe_cex_types::{MarginType, Position, PositionMode, PositionSide};
use serde_json::{Value, json};
use std::{
    future::{Future, poll_fn},
    pin::pin,
    task::Poll,
};
fn endpoint(name: &str) -> &'static str {
    crate::types::ENDPOINTS
        .iter()
        .find(|(key, _)| *key == name)
        .unwrap()
        .1
}
fn first<'a>(row: &'a Value, fields: &[&str]) -> Option<&'a Value> {
    row.as_object()
        .and_then(|r| match get_first_value(r, fields) {
            TableHit::Mapped(v) => Some(v),
            _ => None,
        })
}
fn text(row: &Value, fields: &[&str]) -> Result<String, JsCoercionError> {
    let v = first(row, fields);
    if js_truthy(v) {
        js_string(v)
    } else {
        Ok(String::new())
    }
}
fn numeric(row: &Value, fields: &[&str], fallback: f64) -> Result<f64, JsCoercionError> {
    parse_finite_number(first(row, fields), fallback)
}
fn setting_leverage(v: Option<f64>) -> f64 {
    v.filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(10.0)
}
fn id(symbol: &str, side: PositionSide, mode: PositionMode) -> String {
    if mode == PositionMode::OneWay {
        symbol.into()
    } else {
        format!(
            "{symbol}:{}",
            if side == PositionSide::Long {
                "long"
            } else {
                "short"
            }
        )
    }
}
fn position(
    symbol: String,
    side: PositionSide,
    mode: PositionMode,
    margin: MarginType,
    leverage: f64,
) -> Position {
    Position {
        id: id(&symbol, side, mode),
        symbol,
        side: side.into(),
        entry_price: 0.0,
        notional: 0.0,
        leverage,
        margin_type: (Some(margin)).into(),
        position_mode: (Some(mode)).into(),
        unrealized_pnl: 0.0,
        realized_pnl: (None).into(),
        contracts: 0.0,
        liquidation_price: 0.0,
        margin_size: (None).into(),
        info: (None).into(),
        extra: (None).into(),
        last_update: (None).into(),
        option: (None).into(),
    }
}
pub async fn fetch_positions<E, F>(
    read: impl FnOnce(&'static str, Value) -> F,
    mut settings: impl FnMut(&str) -> (Option<MarginType>, Option<f64>),
    account_mode: impl FnOnce() -> PositionMode,
    markets: impl FnOnce() -> Vec<String>,
    cached: impl FnOnce() -> Vec<Position>,
    report: impl FnOnce(&E),
) -> Vec<Position>
where
    E: From<JsCoercionError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let result: Result<Vec<Position>, E> = async {
        let response = read(endpoint("POSITIONS"), json!({})).await?;
        let mut positions = Vec::new();
        for row in extract_position_rows(response.as_ref()) {
            let symbol = text(row, &["symbol", "instId", "contract", "instrument"])?;
            if symbol.is_empty() {
                continue;
            }
            let contracts = numeric(
                row,
                &[
                    "qty",
                    "maxQty",
                    "size",
                    "positionAmt",
                    "amount",
                    "contracts",
                ],
                0.0,
            )?;
            let mode = if text(row, &["positionMode", "posMode"])?.to_uppercase() == "ONE_WAY" {
                PositionMode::OneWay
            } else {
                PositionMode::Hedged
            };
            let margin = text(row, &["marginMode", "marginType"])?.to_uppercase();
            let (cached_margin, cached_leverage) = settings(&symbol);
            let margin = match margin.as_str() {
                "CROSS" => MarginType::Cross,
                "ISOLATION" | "ISOLATED" => MarginType::Isolated,
                _ => cached_margin.unwrap_or(MarginType::Cross),
            };
            let side = text(row, &["side", "positionSide"])?.to_uppercase();
            let side = if side == "BUY" || side == "LONG" || (side == "BOTH" && contracts >= 0.0) {
                PositionSide::Long
            } else {
                PositionSide::Short
            };
            let mut p = position(
                symbol,
                side,
                mode,
                margin,
                numeric(
                    row,
                    &["leverage", "lever"],
                    setting_leverage(cached_leverage),
                )?,
            );
            p.entry_price = numeric(
                row,
                &["avgOpenPrice", "entryPrice", "openPrice", "avgPrice"],
                0.0,
            )?;
            p.notional = numeric(row, &["entryValue", "notional", "positionValue"], 0.0)?;
            p.unrealized_pnl = numeric(
                row,
                &["unrealizedPNL", "unrealizedPnl", "unRealizedProfit", "upl"],
                0.0,
            )?;
            p.realized_pnl = (Some(numeric(row, &["realizedPNL", "realizedPnl"], 0.0)?)).into();
            p.contracts = contracts.abs();
            p.liquidation_price = numeric(row, &["liqPrice", "liquidationPrice", "liqPx"], 0.0)?;
            p.info = (Some(row.clone())).into();
            positions.push(p);
        }
        let mode = account_mode();
        for symbol in markets() {
            for side in [PositionSide::Long, PositionSide::Short] {
                let (margin, leverage) = settings(&symbol);
                positions.push(position(
                    symbol.clone(),
                    side,
                    mode,
                    margin.unwrap_or(MarginType::Cross),
                    setting_leverage(leverage),
                ));
            }
        }
        let mut seen = std::collections::HashSet::new();
        positions.retain(|p| seen.insert(p.id.clone()));
        Ok(positions)
    }
    .await;
    match result {
        Ok(positions) => positions,
        Err(error) => {
            report(&error);
            cached()
        }
    }
}

pub async fn fetch_orders<O, E1, E2, F1, F2>(
    regular: impl FnOnce() -> F1,
    tpsl: impl FnOnce() -> F2,
) -> (Vec<O>, Option<E1>, Option<E2>)
where
    F1: Future<Output = Result<Vec<O>, E1>>,
    F2: Future<Output = Result<Vec<O>, E2>>,
{
    let mut regular = pin!(regular());
    let mut tpsl = pin!(tpsl());
    let mut regular_result = None;
    let mut tpsl_result = None;
    poll_fn(|cx| {
        if regular_result.is_none() {
            if let Poll::Ready(r) = regular.as_mut().poll(cx) {
                regular_result = Some(r);
            }
        }
        if tpsl_result.is_none() {
            if let Poll::Ready(r) = tpsl.as_mut().poll(cx) {
                tpsl_result = Some(r);
            }
        }
        if regular_result.is_some() && tpsl_result.is_some() {
            Poll::Ready(())
        } else {
            Poll::Pending
        }
    })
    .await;
    let (mut orders, regular_error) = match regular_result.unwrap() {
        Ok(orders) => (orders, None),
        Err(error) => (Vec::new(), Some(error)),
    };
    let tpsl_error = match tpsl_result.unwrap() {
        Ok(tpsl_orders) => {
            orders.extend(tpsl_orders);
            None
        }
        Err(error) => Some(error),
    };
    (orders, regular_error, tpsl_error)
}
