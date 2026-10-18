use crate::order_logic::is_binance_position_mode_already_set_error;
use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_store::Store;
use serde_json::{Value, json};
use std::future::Future;

// ts: BinanceExchange.changePositionMode
/// Configured POST, confirmation and error emission are concrete-class inputs.
pub async fn change_binance_position_mode<F>(
    store: &dyn Store,
    hedged: bool,
    symbol: Option<&str>,
    post: impl FnOnce(&'static str, Value) -> F,
    mut apply_confirmed: impl FnMut(bool, Option<&str>) -> Result<(), ExchangeError>,
    mut emit_error: impl FnMut(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<(), ExchangeError>>,
{
    if store
        .positions()?
        .iter()
        .any(|position| position.contracts > 0.0)
    {
        return emit_error(Value::String(
            "Please close all positions before switching position mode".into(),
        ));
    }
    let result = match post(
        "/fapi/v1/positionSide/dual",
        json!({"dualSidePosition": if hedged { "true" } else { "false" }}),
    )
    .await
    {
        Ok(()) => apply_confirmed(hedged, symbol),
        Err(error) => Err(error),
    };
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            let response = json!({"response":error.response});
            if is_binance_position_mode_already_set_error(Some(&response)) {
                return apply_confirmed(hedged, symbol);
            }
            let message = error
                .response
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
