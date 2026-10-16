//! Spot writes through the caller's unlimited REST adapter.
use safe_cex_exchange::{
    error_reporting::ExchangeErrorInput, exchange::BaseExchange, runtime::ExchangeError,
};
use safe_cex_transport::{RestAdapter, RestRequest};
use serde_json::Value;

// ts: KucoinSpotExchange.cancelAllOrders
pub async fn cancel_all_orders(
    base: &BaseExchange,
    unlimited_xhr: &dyn RestAdapter,
) -> Result<(), Box<dyn std::error::Error>> {
    cancel_request(
        base,
        unlimited_xhr,
        RestRequest {
            method: "DELETE".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[11].1.into(),
            ..Default::default()
        },
        "Failed to cancel all KuCoin Spot orders".into(),
    )
    .await
}

// ts: KucoinSpotExchange.cancelSymbolOrders
pub async fn cancel_symbol_orders(
    base: &BaseExchange,
    unlimited_xhr: &dyn RestAdapter,
    symbol: &str,
) -> Result<(), Box<dyn std::error::Error>> {
    let api_symbol = api_symbol(base, symbol);
    cancel_request(
        base,
        unlimited_xhr,
        RestRequest {
            method: "DELETE".into(),
            url: crate::spot_types::KUCOIN_SPOT_ENDPOINTS[10].1.into(),
            params: Some(serde_json::json!({"symbol":api_symbol})),
            ..Default::default()
        },
        format!("Failed to cancel KuCoin Spot orders for {symbol}"),
    )
    .await
}

fn api_symbol(base: &BaseExchange, symbol: &str) -> String {
    let markets = base.store.markets_by_symbol();
    markets
        .get(symbol)
        .map(|market| market.id.as_str())
        .filter(|id| !id.is_empty())
        .unwrap_or(symbol)
        .to_owned()
}

// ts: KucoinSpotExchange.cancelOrders
pub async fn cancel_orders(
    base: &BaseExchange,
    unlimited_xhr: &dyn RestAdapter,
    orders: &[safe_cex_types::Order],
) -> Result<(), Box<dyn std::error::Error>> {
    for order in orders {
        let symbol = api_symbol(base, &order.symbol);
        cancel_request(
            base,
            unlimited_xhr,
            RestRequest {
                method: "DELETE".into(),
                url: format!(
                    "{}/{}",
                    crate::spot_types::KUCOIN_SPOT_ENDPOINTS[9].1,
                    order.id
                ),
                params: Some(serde_json::json!({"symbol":symbol})),
                ..Default::default()
            },
            format!("Failed to cancel KuCoin Spot order {}", order.id),
        )
        .await?;
    }
    Ok(())
}

async fn cancel_request(
    base: &BaseExchange,
    unlimited_xhr: &dyn RestAdapter,
    request: RestRequest,
    fallback: String,
) -> Result<(), Box<dyn std::error::Error>> {
    let response = unlimited_xhr.execute(request).await;
    let response = match response {
        Ok(response) => response,
        Err(error) => {
            // A failed catch emission escapes with its listener error. Otherwise
            // return the original transport failure with all metadata intact.
            let message = match error.response.as_ref().map(|r| decode(&r.body)).transpose() {
                Err(message) => message,
                Ok(body) => match body
                    .as_ref()
                    .and_then(|b| b.get("msg"))
                    .filter(|v| safe_cex_num::js_truthy(Some(v)))
                {
                    Some(Value::String(message)) => message.clone(),
                    Some(_) => {
                        "KuCoin Spot cancel error emitter requires a string representation".into()
                    }
                    None => error.message.clone(),
                },
            };
            emit(base, message)?;
            return Err(Box::new(error));
        }
    };
    let result = decode(&response.body)
        .map_err(ExchangeError::new)
        .and_then(|data| {
            if data.is_null() {
                let mut error =
                    ExchangeError::new("Cannot read properties of null (reading 'code')");
                error.name = "TypeError".into();
                return Err(error);
            }
            if data.get("code").and_then(Value::as_str) == Some("200000") {
                return Ok(());
            }
            let message = match data.get("msg").filter(|v| safe_cex_num::js_truthy(Some(v))) {
                None => fallback,
                value => match safe_cex_num::js_string(value) {
                    Ok(message) => message,
                    Err(coercion) => {
                        let mut error = ExchangeError::new(coercion.message);
                        error.name = "TypeError".into();
                        return Err(error);
                    }
                },
            };
            Err(ExchangeError::new(message))
        });
    match result {
        Ok(()) => Ok(()),
        Err(error) => {
            emit(base, error.message.clone())?;
            Err(Box::new(error))
        }
    }
}
fn decode(body: &safe_cex_transport::RestResponseBody) -> Result<Value, String> {
    body.json_value()
        .map_err(|e| format!("KuCoin Spot cancel response representation refusal: {e}"))
}
fn emit(base: &BaseExchange, message: String) -> Result<(), ExchangeError> {
    base.emit_and_log_error(ExchangeErrorInput::Message(message), None, None)
}
