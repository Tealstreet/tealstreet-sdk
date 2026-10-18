//! Private MEXC frame handlers. The caller supplies venue-state hooks while
//! the Store and frame normalization remain in this crate.
use crate::utils::{
    from_mexc_symbol, normalize_execution_for_fill, normalize_order, normalize_plan_order,
    normalize_position, normalize_stop_order, to_integer_string, to_number,
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_exchange::websocket::{BaseWebSocket, WsDiagnosticSendOptions};
use safe_cex_store::Store;
use safe_cex_types::{Order, OrderSide, OrderStatus, PositionMode};
use serde::Serialize;
use serde_json::{Value, json};
use std::rc::Rc;

pub fn private_feed_filter_payload() -> Value {
    json!({
        "method":"personal.filter",
        "param":{"filters":[
            {"filter":"order"},
            {"filter":"order.deal"},
            {"filter":"position"},
            {"filter":"asset"},
            {"filter":"plan.order"},
            {"filter":"stop.order"},
            {"filter":"stop.planorder"}
        ]}
    })
}

/// Send the complete MEXC private feed narrowing filter after successful login.
pub fn subscribe_private_feeds(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    let message = serde_json::to_string(&private_feed_filter_payload())
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    socket.send(&message)?;
    socket.emit_ws_diagnostic_send(
        &message,
        &WsDiagnosticSendOptions {
            format: Some(Rc::new(|text| Some(format!("[mexc private] {text}")))),
            redact: None,
        },
    )?;
    Ok(())
}

pub fn private_ping_payload() -> Value {
    json!({"method":"ping"})
}

/// Send the private socket heartbeat frame and record its send timestamp.
pub fn ping_private(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    let message = serde_json::to_string(&private_ping_payload())
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    socket.send(&message)?;
    socket.emit_ws_diagnostic_send(
        &message,
        &WsDiagnosticSendOptions {
            format: Some(Rc::new(|text| Some(format!("[mexc private] {text}")))),
            redact: None,
        },
    )?;
    Ok(())
}

/// Native emitter payload in the source property order; numbers remain binary64.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct MexcFillEvent {
    pub id: String,
    pub order_id: String,
    pub symbol: String,
    pub side: OrderSide,
    pub price: f64,
    pub amount: f64,
    pub fee: f64,
    pub realized_pnl: f64,
}

struct MexcPrivateHandlerHooks {
    note_rejected_regular_order: Rc<dyn Fn(String, f64) -> Result<(), ExchangeError>>,
    mark_stop_like_order_seen: Rc<dyn Fn(String) -> Result<(), ExchangeError>>,
    is_recently_canceled_stop_like_order_id: Rc<dyn Fn(&str) -> Result<bool, ExchangeError>>,
    emit_fill: Rc<dyn Fn(MexcFillEvent) -> Result<(), ExchangeError>>,
}

pub struct MexcPrivateHandlers {
    local_to_utc: Rc<dyn Fn(f64) -> f64>,
    hooks: MexcPrivateHandlerHooks,
}

impl MexcPrivateHandlers {
    /// Source private positions use the normalizer default; the mode argument
    /// remains in this native constructor for caller compatibility.
    pub fn new(
        local_to_utc: Rc<dyn Fn(f64) -> f64>,
        position_mode: Option<PositionMode>,
        note_rejected_regular_order: Rc<dyn Fn(String, f64) -> Result<(), ExchangeError>>,
        mark_stop_like_order_seen: Rc<dyn Fn(String)>,
        is_recently_canceled_stop_like_order_id: Rc<dyn Fn(&str) -> bool>,
        emit_fill: Rc<dyn Fn(MexcFillEvent) -> Result<(), ExchangeError>>,
    ) -> Self {
        Self::new_with_fallible_hooks(
            local_to_utc,
            position_mode,
            note_rejected_regular_order,
            Rc::new(move |id| {
                mark_stop_like_order_seen(id);
                Ok(())
            }),
            Rc::new(move |id| Ok(is_recently_canceled_stop_like_order_id(id))),
            emit_fill,
        )
    }

    /// Native callback adapter; private source parent methods can throw.
    pub fn new_with_fallible_hooks(
        local_to_utc: Rc<dyn Fn(f64) -> f64>,
        _position_mode: Option<PositionMode>,
        note_rejected_regular_order: Rc<dyn Fn(String, f64) -> Result<(), ExchangeError>>,
        mark_stop_like_order_seen: Rc<dyn Fn(String) -> Result<(), ExchangeError>>,
        is_recently_canceled_stop_like_order_id: Rc<dyn Fn(&str) -> Result<bool, ExchangeError>>,
        emit_fill: Rc<dyn Fn(MexcFillEvent) -> Result<(), ExchangeError>>,
    ) -> Self {
        Self {
            local_to_utc,
            hooks: MexcPrivateHandlerHooks {
                note_rejected_regular_order,
                mark_stop_like_order_seen,
                is_recently_canceled_stop_like_order_id,
                emit_fill,
            },
        }
    }

    pub fn handle_order(&self, store: &dyn Store, frame: &Value) -> Result<(), ExchangeError> {
        let rows = checked_source_rows(frame)?;
        let mut orders: Vec<Order> = Vec::new();
        for row in &rows {
            let markets = store.markets_by_symbol()?;
            if row.is_null() {
                return Err(null_property_error("symbol"));
            }
            let raw = row.as_object().cloned().unwrap_or_default();
            let symbol =
                from_mexc_symbol(raw.get("symbol")).map_err(crate::exchange_errors::coercion)?;
            let market = markets.get(&symbol).cloned();
            drop(markets);
            if let Some(order) = normalize_order(&raw, market.as_ref(), self.local_to_utc.as_ref())
                .map_err(crate::exchange_errors::coercion)?
            {
                orders.push(order);
            }
        }
        if !orders.is_empty() {
            // Store writes commit before `?` propagates notification failures.
            store.add_or_update_orders(orders.clone())?;
            for order in &orders {
                if let safe_cex_types::ModelField::Value(
                    status @ (OrderStatus::Closed | OrderStatus::Canceled),
                ) = order.status
                {
                    store.update_order_status(&order.id, status)?;
                }
            }
        }

        for row in &rows {
            let state = row
                .get("state")
                .filter(|value| !value.is_null())
                .or_else(|| row.get("status"));
            let state =
                to_number(state, Some(f64::NAN)).map_err(crate::exchange_errors::coercion)?;
            let error_code = to_number(row.get("errorCode"), Some(0.0))
                .map_err(crate::exchange_errors::coercion)?;
            let deal_volume = to_number(row.get("dealVol"), Some(0.0))
                .map_err(crate::exchange_errors::coercion)?;
            if state == 4.0 && error_code > 0.0 && deal_volume == 0.0 {
                let order_id = to_integer_string(
                    row.get("orderId")
                        .filter(|value| !value.is_null())
                        .or_else(|| row.get("id")),
                )
                .map_err(crate::exchange_errors::coercion)?;
                if !order_id.is_empty() {
                    (self.hooks.note_rejected_regular_order)(order_id, error_code)?;
                }
            }
        }
        Ok(())
    }

    pub fn handle_fill(&self, store: &dyn Store, frame: &Value) -> Result<(), ExchangeError> {
        if frame.is_null() {
            let mut error = ExchangeError::new("Cannot read properties of null (reading 'data')");
            error.name = "TypeError".into();
            return Err(error);
        }
        for row in source_rows(frame) {
            if row.is_null() {
                let mut error =
                    ExchangeError::new("Cannot read properties of null (reading 'symbol')");
                error.name = "TypeError".into();
                return Err(error);
            }
            let raw = row.as_object().cloned().unwrap_or_default();
            // The caller's lookup uses row.symbol only; the normalizer separately
            // accepts row.contract. Giving it a contract-derived market changes qty.
            let symbol =
                from_mexc_symbol(raw.get("symbol")).map_err(crate::exchange_errors::coercion)?;
            let market = store.markets_by_symbol()?.get(&symbol).cloned();
            let Some(fill) =
                normalize_execution_for_fill(&raw, market.as_ref(), self.local_to_utc.as_ref())
                    .map_err(crate::exchange_errors::coercion)?
            else {
                continue;
            };
            let id = fill
                .id
                .filter(|id| !id.is_empty())
                .or_else(|| fill.order_id.clone().filter(|id| !id.is_empty()))
                .unwrap_or_else(|| {
                    format!(
                        "{}:{}",
                        fill.symbol,
                        safe_cex_num::js_number_string(fill.timestamp)
                    )
                });
            (self.hooks.emit_fill)(MexcFillEvent {
                id,
                order_id: fill.order_id.unwrap_or_default(),
                symbol: fill.symbol,
                side: fill.side,
                price: fill.price,
                amount: fill.qty,
                fee: fill.commission.unwrap_or(0.0),
                realized_pnl: fill.realized_pnl.unwrap_or(0.0),
            })?;
        }
        Ok(())
    }

    pub fn handle_position(&self, store: &dyn Store, frame: &Value) -> Result<(), ExchangeError> {
        let rows = checked_source_rows(frame)?;
        let mut positions = Vec::new();
        for row in rows {
            let markets = store.markets_by_symbol()?;
            if row.is_null() {
                return Err(null_property_error("symbol"));
            }
            let raw = row.as_object().cloned().unwrap_or_default();
            let symbol =
                from_mexc_symbol(raw.get("symbol")).map_err(crate::exchange_errors::coercion)?;
            let market = markets.get(&symbol).cloned();
            drop(markets);
            let position =
                normalize_position(&raw, market.as_ref(), None, self.local_to_utc.as_ref())
                    .map_err(crate::exchange_errors::coercion)?;
            if let Some(position) = position {
                positions.push(position);
            }
        }
        if !positions.is_empty() {
            store.add_or_update_positions(positions)?;
        }
        Ok(())
    }

    pub fn handle_plan_order(&self, store: &dyn Store, frame: &Value) -> Result<(), ExchangeError> {
        self.apply_stop_like_frame(store, frame, true)
    }

    pub fn handle_stop_order(&self, store: &dyn Store, frame: &Value) -> Result<(), ExchangeError> {
        self.apply_stop_like_frame(store, frame, false)
    }

    pub fn apply_stop_like_frame(
        &self,
        store: &dyn Store,
        frame: &Value,
        is_plan: bool,
    ) -> Result<(), ExchangeError> {
        let rows = checked_source_rows(frame)?;
        let mut parsed: Vec<(Order, String)> = Vec::new();
        for row in &rows {
            let markets = store.markets_by_symbol()?;
            if row.is_null() {
                return Err(null_property_error("symbol"));
            }
            let raw = row.as_object().cloned().unwrap_or_default();
            let symbol =
                from_mexc_symbol(raw.get("symbol")).map_err(crate::exchange_errors::coercion)?;
            let market = markets.get(&symbol).cloned();
            drop(markets);
            let order = if is_plan {
                normalize_plan_order(&raw, market.as_ref(), self.local_to_utc.as_ref())
            } else {
                normalize_stop_order(&raw, market.as_ref(), self.local_to_utc.as_ref())
            }
            .map_err(crate::exchange_errors::coercion)?;
            if let Some(order) = order {
                parsed.push((order, symbol));
            }
        }

        // Mark source ids before cancellation filtering, even for stale rows.
        for row in &rows {
            let id = row
                .get("id")
                .filter(|value| !value.is_null())
                .or_else(|| row.get("stopPlanOrderId").filter(|value| !value.is_null()))
                .or_else(|| row.get("orderId").filter(|value| !value.is_null()));
            let id = to_integer_string(id).map_err(crate::exchange_errors::coercion)?;
            if !id.is_empty() {
                (self.hooks.mark_stop_like_order_seen)(id)?;
            }
        }

        let mut orders = Vec::new();
        for (order, _symbol) in parsed {
            if order.status == OrderStatus::Open
                && (self.hooks.is_recently_canceled_stop_like_order_id)(&order.id)?
            {
                continue;
            }
            orders.push(order);
        }
        if !orders.is_empty() {
            store.add_or_update_orders(orders)?;
        }
        Ok(())
    }
}

fn source_rows(frame: &Value) -> Vec<Value> {
    let data = frame
        .get("data")
        .filter(|value| safe_cex_num::js_truthy(Some(value)))
        .unwrap_or(frame);
    data.as_array()
        .cloned()
        .unwrap_or_else(|| vec![data.clone()])
}

fn null_property_error(property: &str) -> ExchangeError {
    let mut error = ExchangeError::new(format!(
        "Cannot read properties of null (reading '{property}')"
    ));
    error.name = "TypeError".into();
    error
}

fn checked_source_rows(frame: &Value) -> Result<Vec<Value>, ExchangeError> {
    if frame.is_null() {
        return Err(null_property_error("data"));
    }
    Ok(source_rows(frame))
}
