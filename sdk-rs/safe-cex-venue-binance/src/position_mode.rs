use safe_cex_exchange::runtime::ExchangeError;
use serde_json::Value;
use std::future::Future;

// ts: BinanceExchange.fetchPositionMode
/// `get` is the configured futures xhr.get operation, returning decoded response data.
/// Concrete exchange/client composition remains separate from this extracted method.
pub async fn fetch_binance_position_mode<F, Fut>(get: F) -> Result<bool, ExchangeError>
where
    F: FnOnce(&'static str) -> Fut,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    position_mode(get("/fapi/v1/positionSide/dual").await?)
}

// ts: BinancePMExchange.fetchPositionMode
/// `get` is the configured papiXhr.get operation, returning decoded response data.
pub async fn fetch_binance_pm_position_mode<F, Fut>(get: F) -> Result<bool, ExchangeError>
where
    F: FnOnce(&'static str) -> Fut,
    Fut: Future<Output = Result<Option<Value>, ExchangeError>>,
{
    position_mode(get("/papi/v1/um/positionSide/dual").await?)
}

fn position_mode(data: Option<Value>) -> Result<bool, ExchangeError> {
    if data.is_none() || data == Some(Value::Null) {
        let kind = if data.is_none() { "undefined" } else { "null" };
        let mut error = ExchangeError::new(format!(
            "Cannot read properties of {kind} (reading 'dualSidePosition')"
        ));
        error.name = "TypeError".into();
        return Err(error);
    }
    Ok(data
        .as_ref()
        .and_then(|value| value.get("dualSidePosition"))
        == Some(&Value::Bool(true)))
}

// ts: BinanceExchange.afterMethodCalledInWorker
/// Reconcile the main-thread copy through the supplied concrete confirmation hook.
/// `_result` is absent because the source never inspects the worker result.
pub fn after_binance_method_called_in_worker<E>(
    method: &str,
    args: &[Value],
    apply_confirmed: impl FnOnce(bool, Option<&str>) -> Result<(), E>,
) -> Result<(), E> {
    if method != "changePositionMode" {
        return Ok(());
    }
    apply_confirmed(
        safe_cex_num::js_truthy(args.first()),
        args.get(1).and_then(Value::as_str),
    )
}
