//! Raw ticker read body with configured xhr and fallible emitter seams.
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{
    TableHit,
    get_key::{GetKeyObject, GetKeyValue, v},
    js_string,
};
use serde_json::Value;
use std::future::Future;

fn error_field(data: Option<&Value>) -> Result<Option<&Value>, ExchangeError> {
    if let Some(value @ (Value::Bool(_) | Value::Number(_) | Value::String(_))) = data {
        let mut error = ExchangeError::new(format!(
            "Cannot use 'in' operator to search for 'error' in {}",
            js_string(Some(value)).expect("primitive string coercion is infallible")
        ));
        error.name = "TypeError".into();
        return Err(error);
    }
    match v(
        data.and_then(Value::as_object).map(GetKeyObject::Json),
        "error",
    ) {
        TableHit::Mapped(GetKeyValue::Json(value)) => Ok(Some(value)),
        TableHit::Missing | TableHit::Mapped(GetKeyValue::Undefined) => Ok(None),
        TableHit::Inherited(_) => Err(ExchangeError::new(
            "Phemex error property requires the stock JSON prototype domain",
        )),
    }
}

// ts: PhemexExchange.fetchTickersRaw
pub async fn fetch_tickers_raw<E, F>(
    read: impl FnOnce(&'static str) -> F,
    emit_and_log_error: impl FnOnce(Option<&Value>) -> Result<(), E>,
) -> Result<Value, E>
where
    E: From<ExchangeError>,
    F: Future<Output = Result<Option<Value>, E>>,
{
    let endpoint = crate::constants::ENDPOINTS
        .iter()
        .find(|(name, _)| *name == "TICKERS")
        .expect("Phemex TICKERS constant is present")
        .1;
    let data = read(endpoint).await?;
    // ts-bug: absent error is undefined, which !== null: emit(undefined), then return null.
    if !error_field(data.as_ref())?.is_some_and(Value::is_null) {
        emit_and_log_error(error_field(data.as_ref())?)?;
        return Ok(Value::Null);
    }
    Ok(data.expect("an explicit null error field requires a present response"))
}

/// Production language-value entry. `get_error` is the shared `v` primitive,
/// and the response object is returned by identity on the explicit-null branch.
pub async fn fetch_tickers_raw_value<F>(
    read: impl FnOnce(&'static str) -> F,
    get_error: impl FnOnce(
        &safe_cex_exchange::js_runtime::value::Js,
    ) -> Result<
        safe_cex_exchange::js_runtime::value::Js,
        safe_cex_exchange::js_runtime::value::Thrown,
    >,
    emit: impl FnOnce(
        safe_cex_exchange::js_runtime::value::Js,
    ) -> Result<(), safe_cex_exchange::js_runtime::value::Thrown>,
) -> Result<safe_cex_exchange::js_runtime::value::Js, safe_cex_exchange::js_runtime::value::Thrown>
where
    F: Future<
        Output = Result<
            safe_cex_exchange::js_runtime::value::Js,
            safe_cex_exchange::js_runtime::value::Thrown,
        >,
    >,
{
    use safe_cex_exchange::js_runtime::value::Js;
    let data = read("/md/v3/ticker/24hr/all").await?;
    let error = get_error(&data)?;
    if !error.strict_equals(&Js::Null) {
        emit(error)?;
        return Ok(Js::Null);
    }
    Ok(data)
}
