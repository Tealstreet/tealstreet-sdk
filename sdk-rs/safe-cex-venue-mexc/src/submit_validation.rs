//! Submission rejection bodies, retaining the original live response payload.
use crate::{
    exchange_helpers::{extract_rows, note_regular_order_submit_error},
    utils::{to_integer_string, to_number},
};
use safe_cex_num::{JsCoercionError, js_string, js_truthy};
use serde_json::{Value, json};
use std::{cell::RefCell, rc::Rc};

/// Native representation of the errors created by the two assertion bodies.
/// `response_data` aliases the supplied payload, matching error.response.data.
#[derive(Debug)]
pub struct MexcOrderSubmitError {
    pub name: &'static str,
    pub message: String,
    pub response_data: Option<Rc<RefCell<Value>>>,
    pub partial_order_ids: Option<Vec<String>>,
    pub partial_order_count: Option<usize>,
    pub total_order_count: Option<f64>,
    pub failed_order_index: Option<usize>,
}

fn error(message: String) -> MexcOrderSubmitError {
    MexcOrderSubmitError {
        name: "Error",
        message,
        response_data: None,
        partial_order_ids: None,
        partial_order_count: None,
        total_order_count: None,
        failed_order_index: None,
    }
}
fn coercion(value: JsCoercionError) -> MexcOrderSubmitError {
    let mut result = error(value.message.into());
    result.name = value.name;
    result
}
fn business_error(
    payload: &Rc<RefCell<Value>>,
    action: &str,
) -> Result<MexcOrderSubmitError, MexcOrderSubmitError> {
    let data = payload.borrow();
    let detail = ["message", "msg"]
        .iter()
        .filter_map(|key| data.get(*key))
        .find(|v| js_truthy(Some(v)));
    let message = match detail {
        Some(value) => js_string(Some(value)).map_err(coercion)?,
        None => format!(
            "{action} failed with code {}",
            js_string(data.get("code")).map_err(coercion)?
        ),
    };
    let mut result = error(message);
    result.response_data = Some(payload.clone());
    Ok(result)
}

// ts: MexcExchange.assertRegularOrderSubmitSucceeded
pub fn assert_regular_order_submit_succeeded(
    payload: Option<&Rc<RefCell<Value>>>,
    cooldown_until: &mut f64,
    now: &dyn Fn() -> f64,
) -> Result<(), MexcOrderSubmitError> {
    if !payload.is_some_and(|v| v.borrow().get("success") == Some(&Value::Bool(false))) {
        return Ok(());
    }
    let payload = payload.expect("false success belongs to a payload");
    let result = business_error(payload, "MEXC order placement")?;
    // Only a read-only helper view is copied. Release its temporary input borrow
    // before the clock hook; the returned error retains the live original handle.
    let view = json!({"message": result.message, "response": {"data": &*payload.borrow()}});
    note_regular_order_submit_error(cooldown_until, Some(&view), now).map_err(coercion)?;
    Err(result)
}

fn row_code(row: &Value) -> Result<f64, MexcOrderSubmitError> {
    if row.is_null() {
        let mut result = error("Cannot read properties of null (reading 'errorCode')".into());
        result.name = "TypeError";
        return Err(result);
    }
    to_number(row.get("errorCode"), Some(0.0)).map_err(coercion)
}

// ts: MexcExchange.assertRegularOrderBatchSucceeded
pub fn assert_regular_order_batch_succeeded(
    payload: Option<&Rc<RefCell<Value>>>,
    total_order_count: f64,
) -> Result<(), MexcOrderSubmitError> {
    if payload.is_some_and(|v| v.borrow().get("success") == Some(&Value::Bool(false))) {
        return Err(business_error(
            payload.expect("false success payload"),
            "MEXC batch order placement",
        )?);
    }
    let data = payload.map(|v| v.borrow());
    let rows = extract_rows(data.as_deref(), &[]);
    let mut failed_rows = Vec::new();
    // Complete filtering before any ID/message formatting: later null rows or
    // invalid code coercion must win over earlier failed-message/ID coercion.
    for (index, row) in rows.iter().enumerate() {
        if row_code(row)? != 0.0 {
            failed_rows.push((index, row));
        }
    }
    if failed_rows.is_empty() {
        return Ok(());
    }

    let mut partial_order_ids = Vec::new();
    for row in rows {
        if row_code(row)? == 0.0 {
            let id = row
                .get("orderId")
                .filter(|v| !v.is_null())
                .or_else(|| row.get("id"));
            let id = to_integer_string(id).map_err(coercion)?;
            if !id.is_empty() {
                partial_order_ids.push(id);
            }
        }
    }
    let mut messages = Vec::new();
    for (index, (_, row)) in failed_rows.iter().enumerate() {
        let detail = ["errorMsg", "message", "msg"]
            .iter()
            .filter_map(|key| row.get(*key))
            .find(|v| js_truthy(Some(v)))
            .or_else(|| row.get("errorCode"));
        messages.push(format!(
            "{}: {}",
            index + 1,
            js_string(detail).map_err(coercion)?
        ));
    }
    let mut result = error(format!(
        "MEXC batch order placement failed: {}",
        messages.join(", ")
    ));
    if !partial_order_ids.is_empty() {
        result.partial_order_count = Some(partial_order_ids.len());
        result.partial_order_ids = Some(partial_order_ids);
        result.total_order_count = Some(total_order_count);
        result.failed_order_index = Some(failed_rows[0].0);
    }
    Err(result)
}
