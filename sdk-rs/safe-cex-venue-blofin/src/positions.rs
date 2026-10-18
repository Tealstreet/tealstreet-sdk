//! BloFin position projection and its warning-before-mapping boundary.
use safe_cex_num::{
    TableHit, adjust, js_table_lookup, multiply,
    numeric_ingress::{JsNumberOutput, js_value_as_f64},
    parse_float,
};
use safe_cex_store::Store;
use safe_cex_types::{
    MarginType, Position, PositionExtra, PositionMode, PositionSide,
    js_coercion::{JsCoercionError, js_string},
    model_utils::create_position_id,
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PositionsInput {
    #[serde(deserialize_with = "safe_cex_num::numeric_ingress::js_deserialize_passthrough")]
    value: Value,
}
impl JsNumberOutput for PositionsInput {
    fn check_number_output(&self) {
        safe_cex_num::js_number_output!(self, struct {} passthrough { value });
    }
}
#[derive(Debug)]
pub enum PositionMapError<E> {
    Coercion(JsCoercionError),
    Emitter(E),
}
impl<E> From<JsCoercionError> for PositionMapError<E> {
    fn from(e: JsCoercionError) -> Self {
        Self::Coercion(e)
    }
}
fn error(message: &'static str) -> JsCoercionError {
    JsCoercionError {
        name: "TypeError",
        message,
    }
}
fn representation() -> JsCoercionError {
    JsCoercionError {
        name: "RepresentationError",
        message: "Source position is outside the shared typed Position model",
    }
}
fn number(v: Option<&Value>) -> Result<f64, JsCoercionError> {
    Ok(parse_float(&js_string(v)?))
}
fn truthy(v: f64) -> bool {
    v != 0.0 && !v.is_nan()
}
// The nested source predicate uses ===, including reference identity for objects.
fn strict_equal(a: Option<&Value>, b: Option<&Value>) -> bool {
    match (a, b) {
        (None, None) => true,
        (Some(Value::Number(_)), Some(Value::Number(_))) => {
            js_value_as_f64(a.unwrap()) == js_value_as_f64(b.unwrap())
        }
        (Some(a @ (Value::Object(_) | Value::Array(_))), Some(b)) => std::ptr::eq(a, b),
        (Some(a), Some(b)) => a == b,
        _ => false,
    }
}
pub fn map_positions<E>(
    data: &PositionsInput,
    store: &dyn Store,
    mut emit_and_log_error: impl FnMut(&str) -> Result<(), E>,
) -> Result<Vec<Position>, PositionMapError<E>> {
    let rows = data.value.as_array().ok_or_else(|| {
        error(if data.value.is_null() {
            "Cannot read properties of null (reading 'filter')"
        } else {
            "data.filter is not a function"
        })
    })?;
    let mut supported = Vec::new();
    for row in rows {
        if !crate::utility::is_known_non_swap_instrument(
            &crate::utility::UtilityInput::from_retained((row.clone()).into()),
        )? {
            supported.push(row);
        }
    }
    if supported.iter().any(|p| {
        supported.iter().any(|p2| {
            (!strict_equal(p2.get("marginMode"), p.get("marginMode"))
                || (p2.get("positionSide").and_then(Value::as_str) == Some("net")
                    && p.get("positionSide").and_then(Value::as_str) != Some("net")))
                && strict_equal(p.get("instId"), p2.get("instId"))
                && strict_equal(p.get("positionSide"), p2.get("positionSide"))
                && !strict_equal(p.get("positions"), p2.get("positions"))
        })
    }) {
        let key = js_string(rows[0].get("instId"))?;
        emit_and_log_error(&format!("Isolated and cross positions open at the same time for {key}. Close one on Blofin site to use Tealstreet.")).map_err(PositionMapError::Emitter)?;
    }
    let mut out = Vec::new();
    for p in supported {
        let key = js_string(p.get("instId"))?;
        let markets = store.markets_by_id();
        let Some(market) = markets.get(&key) else {
            if matches!(
                js_table_lookup::<&str, ()>(&[], &key),
                TableHit::Inherited(_)
            ) {
                number(p.get("positions"))?;
                return Err(
                    error("Cannot read properties of undefined (reading 'contractValue')").into(),
                );
            }
            continue;
        };
        let pos = number(p.get("positions"))?;
        let contract = if market.contract_size.is_some_and(truthy) {
            market.contract_size.unwrap()
        } else {
            let info = market.info.as_ref().ok_or_else(|| {
                error("Cannot read properties of undefined (reading 'contractValue')")
            })?;
            if info.is_nullish() {
                return Err(error(if info.is_null() {
                    "Cannot read properties of null (reading 'contractValue')"
                } else {
                    "Cannot read properties of undefined (reading 'contractValue')"
                })
                .into());
            }
            let n = safe_cex_num::js_retained_parse_float(info.get("contractValue"))?;
            if truthy(n) { n } else { 1.0 }
        };
        let contracts = if truthy(pos) && truthy(contract) {
            multiply(pos, contract)
        } else {
            0.0
        };
        let mark_price = number(p.get("markPrice"))?;
        let notional_usd = multiply(contracts, mark_price);
        let side_key = js_string(p.get("positionSide"))?;
        let mut side = match crate::types::POSITION_SIDE
            .get(&side_key)
            .and_then(Value::as_str)
        {
            Some("long") => Some(PositionSide::Long),
            Some("short") => Some(PositionSide::Short),
            _ => None,
        };
        let inherited = matches!(
            js_table_lookup::<&str, ()>(&[], &side_key),
            TableHit::Inherited(_)
        );
        if contracts > 0.0 {
            side = Some(PositionSide::Long);
        }
        if contracts < 0.0 {
            side = Some(PositionSide::Short);
        }
        if side.is_none() && inherited {
            return Err(representation().into());
        }
        let mut side = side.unwrap_or(PositionSide::Long);
        let mode = if p.get("positionSide").and_then(Value::as_str) == Some("net") {
            PositionMode::OneWay
        } else {
            PositionMode::Hedged
        };
        match p.get("positionSide").and_then(Value::as_str) {
            Some("short") => side = PositionSide::Short,
            Some("long") => side = PositionSide::Long,
            _ => {}
        }
        let id = create_position_id(&market.symbol, Some(side), Some(mode));
        let positions = store.positions();
        let existing = positions.iter().find(|sp| sp.id == id);
        let parsed_leverage = number(p.get("leverage"))?;
        let mut leverage = if parsed_leverage.is_finite() && parsed_leverage > 0.0 {
            parsed_leverage
        } else {
            0.0
        };
        if !truthy(leverage) {
            if let Some(existing) = existing.filter(|p| p.leverage > 0.0) {
                leverage = existing.leverage;
            } else {
                let fetched = store.fetched_leverage();
                if let Some(fetched) = fetched.get(&market.symbol) {
                    if fetched.is_fetching != Some(true) {
                        leverage = if side == PositionSide::Long {
                            fetched.buy_leverage
                        } else {
                            fetched.sell_leverage
                        };
                    }
                } else if matches!(
                    js_table_lookup::<&str, ()>(&[], &market.symbol),
                    TableHit::Inherited(_)
                ) {
                    return Err(representation().into());
                }
            }
        }
        let parsed_liquidation = number(p.get("liquidationPrice"))?;
        let liquidation = if parsed_liquidation.is_finite() {
            parsed_liquidation
        } else {
            existing
                .map(|p| p.liquidation_price)
                .filter(|v| truthy(*v))
                .unwrap_or(0.0)
        };
        out.push(Position {
            id,
            symbol: market.symbol.clone(),
            side: side.into(),
            entry_price: if truthy(contracts) {
                number(p.get("averagePrice"))?
            } else {
                0.0
            },
            notional: if truthy(contracts) {
                adjust(
                    parse_float(&safe_cex_num::js_number_string(notional_usd)),
                    market.precision.price,
                )
                .abs()
            } else {
                0.0
            },
            leverage,
            unrealized_pnl: if truthy(contracts) {
                adjust(number(p.get("unrealizedPnl"))?, market.precision.price)
            } else {
                0.0
            },
            contracts: contracts.abs(),
            liquidation_price: liquidation,
            margin_type: (Some(
                if p.get("marginMode").and_then(Value::as_str) == Some("isolated") {
                    MarginType::Isolated
                } else {
                    MarginType::Cross
                },
            ))
            .into(),
            position_mode: (Some(mode)).into(),
            extra: (Some(PositionExtra {
                contracts_native: Some(pos.abs()),
                unrealized_pnl_native: None,
            }))
            .into(),
            info: (Some(p.clone().into())).into(),
            realized_pnl: (None).into(),
            margin_size: (None).into(),
            last_update: (None).into(),
            option: (None).into(),
        });
    }
    Ok(out)
}

impl PositionsInput {
    pub(crate) fn from_retained(value: Value) -> Self {
        Self { value }
    }
}
