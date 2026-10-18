use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::{Store, StoreMutationError};
use serde_json::{Value, json};
use std::future::Future;

// ts: BinancePMExchange.changePositionMode
/// Configured POST and error emission remain concrete-class composition inputs.
/// Store notification failures enter the same source catch as POST failures.
pub async fn change_binance_pm_position_mode<F>(
    store: &dyn Store,
    hedged: bool,
    post: impl FnOnce(&'static str, Value) -> F,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<(), ExchangeError>>,
{
    let has_open_position = store
        .positions()
        .iter()
        .any(|position| position.contracts > 0.0);
    if has_open_position {
        return emit_error(Value::String(
            "Please close all positions before switching position mode".into(),
        ));
    }
    let result = match post(
        "/papi/v1/um/positionSide/dual",
        json!({"dualSidePosition": if hedged { "true" } else { "false" }}),
    )
    .await
    {
        Ok(()) => {
            let mutation: Result<(), StoreMutationError> = store.set_setting("isHedged", hedged);
            mutation.map_err(|error: StoreMutationError| ExchangeError::from(error))
        }
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let message = error
                .response_json_view()
                .as_ref()
                .and_then(|response| response.get("data"))
                .and_then(|data| data.get("msg"))
                .filter(|value| safe_cex_num::js_truthy(Some(value)))
                .cloned()
                .unwrap_or(Value::String(error.message));
            emit_error(message)
        }
    }
}
