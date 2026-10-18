use crate::payload::{StoreWorkerOp, StoreWorkerPayload};
use safe_cex_exchange::exchange::Exchange;
use safe_cex_types::ModelField;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::HashSet,
    rc::Rc,
};

pub struct UpdatePositionsHandler {
    exchange: Rc<dyn Exchange>,
    post: Rc<dyn Fn(StoreWorkerPayload)>,
    last_failed_request: Cell<f64>,
    requests_in_progress: RefCell<HashSet<Option<String>>>,
}
struct RequestGuard<'a> {
    handler: &'a UpdatePositionsHandler,
    symbols: Option<Vec<String>>,
    request_id: String,
    settled: bool,
}
impl Drop for RequestGuard<'_> {
    fn drop(&mut self) {
        let mut pending = self.handler.requests_in_progress.borrow_mut();
        match &self.symbols {
            Some(symbols) => {
                for symbol in symbols {
                    pending.remove(&Some(symbol.clone()));
                }
            }
            None => {
                pending.remove(&None);
            }
        }
        drop(pending);
        // Cancellation must release only this request's slots and settle its bridge waiter.
        if !self.settled {
            self.handler.done(
                &self.request_id,
                false,
                Some("Worker request cancelled".into()),
            );
        }
    }
}
impl UpdatePositionsHandler {
    pub fn new(exchange: Rc<dyn Exchange>, post: Rc<dyn Fn(StoreWorkerPayload)>) -> Rc<Self> {
        Rc::new(Self {
            exchange,
            post,
            last_failed_request: Cell::new(0.0),
            requests_in_progress: RefCell::default(),
        })
    }
    pub async fn handle_update_positions(&self, request_id: String, symbols: Option<Vec<String>>) {
        let now = self.exchange.base().runtime.now_ms();
        if now - self.last_failed_request.get() < 5000.0 {
            self.done(&request_id, false, Some("retry_too_soon".into()));
            return;
        }
        let valid = symbols.as_ref().map(|symbols| {
            symbols
                .iter()
                .filter(|symbol| {
                    !self
                        .requests_in_progress
                        .borrow()
                        .contains(&Some((*symbol).clone()))
                })
                .cloned()
                .collect::<Vec<_>>()
        });
        if valid.as_ref().is_some_and(Vec::is_empty)
            || (valid.is_none() && self.requests_in_progress.borrow().contains(&None))
        {
            self.done(&request_id, true, None);
            return;
        }
        {
            let mut pending = self.requests_in_progress.borrow_mut();
            match &valid {
                Some(symbols) => pending.extend(symbols.iter().cloned().map(Some)),
                None => {
                    pending.insert(None);
                }
            }
        }
        let mut guard = RequestGuard {
            handler: self,
            symbols: valid,
            request_id: request_id.clone(),
            settled: false,
        };
        // ts-bug: locks use filtered symbols but the fetch receives the original list.
        let result = self
            .exchange
            .fetch_positions(symbols)
            .await
            .and_then(|positions| {
                if self.exchange.base().is_disposed() {
                    return Ok(false);
                }
                if !positions_equal(&positions, &self.exchange.base().store.positions()?) {
                    if guard.symbols.is_none() {
                        self.exchange
                            .base()
                            .store
                            .add_or_update_positions_full(positions)?;
                    } else if !positions.is_empty() {
                        self.exchange
                            .base()
                            .store
                            .add_or_update_positions(positions)?;
                    }
                }
                Ok(true)
            });
        match result {
            Ok(true) => self.done(&request_id, true, None),
            Ok(false) => {}
            Err(error) => {
                self.last_failed_request.set(now);
                self.done(&request_id, false, Some(error.js_error_string()));
            }
        }
        guard.settled = true;
    }
    pub fn handle(self: &Rc<Self>, payload: StoreWorkerPayload) {
        if payload.op != StoreWorkerOp::UpdatePositions {
            return;
        }
        let data = payload.data.unwrap_or(Value::Null);
        let request_id = data["requestId"].as_str().unwrap_or("").to_owned();
        let symbols = data.get("symbols").and_then(Value::as_array).map(|values| {
            values
                .iter()
                .filter_map(Value::as_str)
                .map(str::to_owned)
                .collect()
        });
        let handler = self.clone();
        self.exchange
            .base()
            .runtime
            .spawn_local(Box::pin(async move {
                handler.handle_update_positions(request_id, symbols).await;
            }));
    }
    fn done(&self, request_id: &str, success: bool, reason: Option<String>) {
        let mut data = json!({"requestId":request_id,"success":success});
        if let Some(reason) = reason {
            data["reason"] = json!(reason);
        }
        (self.post)(StoreWorkerPayload {
            op: StoreWorkerOp::UpdatePositionsDone,
            instance_id: None,
            data: Some(data),
        });
    }
}

fn positions_equal(left: &[safe_cex_types::Position], right: &[safe_cex_types::Position]) -> bool {
    fn number(a: f64, b: f64) -> bool {
        a == b || (a.is_nan() && b.is_nan())
    }
    // lodash isEqual counts own keys, so an own undefined differs from an absent one.
    fn field<T>(a: &ModelField<T>, b: &ModelField<T>, eq: impl Fn(&T, &T) -> bool) -> bool {
        match (a, b) {
            (ModelField::Value(a), ModelField::Value(b)) => eq(a, b),
            (ModelField::Absent, ModelField::Absent)
            | (ModelField::Undefined, ModelField::Undefined) => true,
            _ => false,
        }
    }
    fn optional(a: Option<f64>, b: Option<f64>) -> bool {
        match (a, b) {
            (Some(a), Some(b)) => number(a, b),
            (None, None) => true,
            _ => false,
        }
    }
    let num = |a: &f64, b: &f64| number(*a, *b);
    left.len() == right.len()
        && left.iter().zip(right).all(|(a, b)| {
            a.id == b.id
                && a.symbol == b.symbol
                && a.side == b.side
                && number(a.entry_price, b.entry_price)
                && number(a.notional, b.notional)
                && number(a.leverage, b.leverage)
                && a.margin_type == b.margin_type
                && a.position_mode == b.position_mode
                && number(a.unrealized_pnl, b.unrealized_pnl)
                && field(&a.realized_pnl, &b.realized_pnl, num)
                && number(a.contracts, b.contracts)
                && number(a.liquidation_price, b.liquidation_price)
                && field(&a.margin_size, &b.margin_size, num)
                && field(&a.last_update, &b.last_update, num)
                && field(&a.info, &b.info, retained_equal)
                && field(&a.extra, &b.extra, |a, b| {
                    optional(a.contracts_native, b.contracts_native)
                        && optional(a.unrealized_pnl_native, b.unrealized_pnl_native)
                })
        })
}
fn retained_equal(
    a: &safe_cex_types::retained_json::RetainedJsonInput,
    b: &safe_cex_types::retained_json::RetainedJsonInput,
) -> bool {
    use safe_cex_types::retained_json::RetainedJsonInput as Input;
    match (a, b) {
        (Input::Float(_) | Input::Lexeme(_), Input::Float(_) | Input::Lexeme(_)) => a
            .as_f64()
            .zip(b.as_f64())
            .is_some_and(|(a, b)| a == b || (a.is_nan() && b.is_nan())),
        (Input::Array(a), Input::Array(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(a, b)| retained_equal(a, b))
        }
        (Input::Object(a), Input::Object(b)) => {
            let a: std::collections::HashMap<&[u16], _> = a
                .iter()
                .map(|(key, value)| (key.as_slice(), value))
                .collect();
            let b: std::collections::HashMap<&[u16], _> = b
                .iter()
                .map(|(key, value)| (key.as_slice(), value))
                .collect();
            a.len() == b.len()
                && a.iter()
                    .all(|(key, a)| b.get(key).is_some_and(|b| retained_equal(a, b)))
        }
        _ => a == b,
    }
}
