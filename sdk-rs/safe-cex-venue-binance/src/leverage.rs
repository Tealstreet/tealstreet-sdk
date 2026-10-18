use safe_cex_exchange::runtime::ExchangeError;
use safe_cex_num::{TableHit, js_table_lookup, math_max, math_min};
use safe_cex_store::{Store, model_patch::ModelPatch};
use safe_cex_types::{PositionSide, model_utils::create_position_id};
use serde_json::Value;
use std::future::Future;

/// Numeric payload remains native until the configured client's wire boundary.
#[derive(Debug, Clone, PartialEq)]
pub struct BinanceLeverageRequest {
    pub symbol: String,
    pub leverage: f64,
}

// ts: BinanceExchange.setLeverage
/// `post` is the configured futures xhr.post; `emit_error` is emitAndLogError.
pub async fn set_binance_leverage<F>(
    store: &dyn Store,
    symbol: &str,
    input_leverage: f64,
    post: impl FnOnce(&'static str, BinanceLeverageRequest) -> F,
    emit_error: impl FnOnce(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<(), ExchangeError>>,
{
    set_leverage(
        store,
        symbol,
        input_leverage,
        "/fapi/v1/leverage",
        post,
        emit_error,
    )
    .await
}

// ts: BinancePMExchange.setLeverage
/// `post` is the configured papiXhr.post, not the futures client.
pub async fn set_binance_pm_leverage<F>(
    store: &dyn Store,
    symbol: &str,
    input_leverage: f64,
    post: impl FnOnce(&'static str, BinanceLeverageRequest) -> F,
    emit_error: impl FnOnce(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<(), ExchangeError>>,
{
    set_leverage(
        store,
        symbol,
        input_leverage,
        "/papi/v1/um/leverage",
        post,
        emit_error,
    )
    .await
}

async fn set_leverage<F>(
    store: &dyn Store,
    symbol: &str,
    input_leverage: f64,
    endpoint: &'static str,
    post: impl FnOnce(&'static str, BinanceLeverageRequest) -> F,
    emit_error: impl FnOnce(Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<(), ExchangeError>>,
{
    let market = store.markets_by_symbol()?.get(symbol).cloned();
    let position = store
        .positions()?
        .iter()
        .find(|p| p.symbol == symbol)
        .cloned();
    // Source marketsBySymbol is an ordinary object. An inherited truthy market
    // passes the first guard, then fails only after the position guard.
    let inherited_market = market.is_none()
        && matches!(
            js_table_lookup(&[] as &[(&str, ())], symbol),
            TableHit::Inherited(_)
        );
    if market.is_none() && !inherited_market {
        return Err(ExchangeError::new(format!("Market {symbol} not found")));
    }
    let position =
        position.ok_or_else(|| ExchangeError::new(format!("Position {symbol} not found")))?;
    let Some(market) = market else {
        let mut error =
            ExchangeError::new("Cannot read properties of undefined (reading 'leverage')");
        error.name = "TypeError".into();
        return Err(error);
    };
    let leverage = math_min(
        math_max(input_leverage, market.limits.leverage.min),
        market.limits.leverage.max,
    );
    if position.leverage == leverage {
        return Ok(());
    }
    let result = match post(
        endpoint,
        BinanceLeverageRequest {
            symbol: symbol.into(),
            leverage,
        },
    )
    .await
    {
        Ok(()) => {
            let mut patch = ModelPatch::default();
            patch.set_number("leverage", leverage);
            store
                .update_positions(&[
                    (
                        Some(create_position_id(symbol, Some(PositionSide::Long), None)),
                        patch.clone(),
                    ),
                    (
                        Some(create_position_id(symbol, Some(PositionSide::Short), None)),
                        patch,
                    ),
                ])
                .map_err(ExchangeError::from)
        }
        Err(error) => Err(error),
    };
    if let Err(error) = result {
        let message = error
            .response_json_view()
            .as_ref()
            .and_then(|r| r.get("data"))
            .and_then(|d| d.get("msg"))
            .filter(|msg| safe_cex_num::js_truthy(Some(msg)))
            .cloned()
            .unwrap_or(Value::String(error.message));
        emit_error(message)?;
    }
    Ok(())
}
