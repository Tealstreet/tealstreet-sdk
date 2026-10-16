//! Exchange execution wrappers with explicit async host dependencies. Lifecycle
//! and transport composition remain separate manifest rows.
use crate::order_params::{
    BybitOrderExecutionParams, BybitPlaceOrderOpts, get_bybit_order_execution_params,
};
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_types::PlaceOrderOpts;
use serde_json::{Map, Value};
use std::{
    borrow::Borrow,
    cell::{Cell, RefCell},
    future::Future,
    rc::Rc,
};

pub async fn reinit_on_throw<T, E>(
    needs_reinit: &Cell<bool>,
    request: impl Future<Output = Result<T, E>>,
) -> Result<T, E> {
    match request.await {
        Ok(value) => Ok(value),
        Err(error) => {
            needs_reinit.set(true);
            Err(error)
        }
    }
}

pub fn get_inverse_execution_query_windows(
    from: Option<f64>,
    to: Option<f64>,
    max_range_ms: f64,
) -> Vec<(Option<f64>, Option<f64>)> {
    let (Some(start), Some(end)) = (from, to) else {
        return vec![(from, to)];
    };
    if end - start <= max_range_ms {
        return vec![(from, to)];
    }
    let mut windows = Vec::new();
    let mut window_end = end;
    while window_end > start {
        let window_start = safe_cex_num::math_max(start, window_end - max_range_ms);
        windows.push((Some(window_start), Some(window_end)));
        if window_start <= start {
            break;
        }
        window_end = window_start - 1.;
    }
    windows
}

fn report_execution_result(
    result: Result<BybitOrderExecutionParams, Rc<ExchangeError>>,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), Rc<ExchangeError>>,
) -> Result<BybitOrderExecutionParams, Rc<ExchangeError>> {
    match result {
        Ok(value) => Ok(value),
        Err(error) => {
            let message = if error.message.is_empty() {
                error.js_error_string()
            } else {
                error.message.clone()
            };
            emit_and_log_error(&message)?;
            Err(error)
        }
    }
}

pub fn get_order_execution_params(
    opts: &BybitPlaceOrderOpts,
    get_category_for_symbol: impl FnOnce(&str) -> Result<String, Rc<ExchangeError>>,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), Rc<ExchangeError>>,
) -> Result<BybitOrderExecutionParams, Rc<ExchangeError>> {
    let base: &PlaceOrderOpts = opts.borrow();
    let result = get_category_for_symbol(&base.symbol).and_then(|category| {
        get_bybit_order_execution_params(opts, &category)
            .map_err(|message| Rc::new(ExchangeError::new(message)))
    });
    report_execution_result(result, emit_and_log_error)
}

pub fn spot_get_order_execution_params(
    opts: &BybitPlaceOrderOpts,
    emit_and_log_error: impl FnOnce(&str) -> Result<(), Rc<ExchangeError>>,
) -> Result<BybitOrderExecutionParams, Rc<ExchangeError>> {
    let result = get_bybit_order_execution_params(opts, "spot")
        .map_err(|message| Rc::new(ExchangeError::new(message)));
    report_execution_result(result, emit_and_log_error)
}

pub async fn get_order_position_idx<F: Future<Output = Result<Option<Value>, E>>, E>(
    opts: &RefCell<Map<String, Value>>,
    fetch_position_mode: impl FnOnce(Option<Value>) -> F,
) -> Result<f64, E> {
    let symbol = opts.borrow().get("symbol").cloned();
    let position_mode = fetch_position_mode(symbol).await?;
    if position_mode.as_ref().and_then(Value::as_str) == Some("oneway") {
        return Ok(0.);
    }
    // Both properties are read after await, as in the maintained arrow body.
    let opts = opts.borrow();
    let mut index = if opts.get("side").and_then(Value::as_str) == Some("buy") {
        1.
    } else {
        2.
    };
    if safe_cex_num::js_truthy(opts.get("reduceOnly")) {
        index = if index == 1. { 2. } else { 1. };
    }
    Ok(index)
}

pub async fn spot_get_order_position_idx<T>(_opts: &T) -> f64 {
    0.
}

pub async fn get_stop_order_position_idx<F: Future<Output = Result<f64, E>>, E>(
    opts: &Map<String, Value>,
    get_order_position_idx: impl FnOnce(Map<String, Value>) -> F,
) -> Result<Option<f64>, E> {
    let mut without_reduce_only = opts.clone();
    without_reduce_only.remove("reduceOnly");
    let index = get_order_position_idx(without_reduce_only).await?;
    Ok(if index == 0. {
        Some(0.)
    } else if index == 1. {
        Some(2.)
    } else if index == 2. {
        Some(1.)
    } else {
        None
    })
}
