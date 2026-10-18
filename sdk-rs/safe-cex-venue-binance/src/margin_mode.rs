use safe_cex_exchange::runtime::ExchangeError;

// ts: BinancePMExchange.changeMarginType
/// Source arguments are unused. The fallible callback is the concrete
/// emitAndLogError operation; a listener error takes precedence over the
/// unsupported-margin error. Concrete exchange composition remains pending.
pub async fn change_binance_pm_margin_type(
    emit_and_log_error: impl FnOnce(&str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let message = "Portfolio margin is not supported";
    emit_and_log_error(message)?;
    Err(ExchangeError::new(message))
}

/// Native request body; the wire layer maps the enum to ISOLATED or CROSSED.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BinanceMarginType {
    Isolated,
    Crossed,
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BinanceMarginTypeRequest {
    pub symbol: String,
    pub margin_type: BinanceMarginType,
}

// ts: BinanceExchange.changeMarginType
/// Extracted async body. POST is configured xhr.post; emitter is configured
/// emitAndLogError. Normalized ExchangeError and JSON message values are explicit
/// composition inputs; executable getters/coercions, synchronous callback throws
/// and arbitrary emitter mutation of the rejected object are outside this boundary.
/// Symbol guard precedes POST; an emitter error escapes before fresh Error creation.
pub async fn change_binance_margin_type<F>(
    isolated: bool,
    symbol: Option<&str>,
    post: impl FnOnce(&'static str, BinanceMarginTypeRequest) -> F,
    mut emit_error: impl FnMut(serde_json::Value) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError>
where
    F: std::future::Future<Output = Result<(), ExchangeError>>,
{
    let Some(symbol) = symbol.filter(|symbol| !symbol.is_empty()) else {
        emit_error(serde_json::Value::String("Symbol is required".into()))?;
        return Err(ExchangeError::new("Symbol is required"));
    };
    if let Err(error) = post(
        "/fapi/v1/marginType",
        BinanceMarginTypeRequest {
            symbol: symbol.into(),
            margin_type: if isolated {
                BinanceMarginType::Isolated
            } else {
                BinanceMarginType::Crossed
            },
        },
    )
    .await
    {
        let message = error
            .response_json_view()
            .as_ref()
            .and_then(|response| response.get("data"))
            .and_then(|data| data.get("msg"))
            .filter(|value| safe_cex_num::js_truthy(Some(value)))
            .cloned()
            .unwrap_or(serde_json::Value::String(error.message));
        emit_error(message.clone())?;
        let text = safe_cex_num::js_string(Some(&message)).map_err(|error| {
            let mut result = ExchangeError::new(error.message);
            result.name = error.name.into();
            result
        })?;
        return Err(ExchangeError::new(text));
    }
    Ok(())
}
