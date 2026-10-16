use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::{cell::Cell, future::Future, rc::Rc};

// ts: BinancePrivateWebsocket.onOpen (binancespot.ws-private.ts)
pub async fn open_binance_spot_private<F>(
    socket: &Rc<BaseWebSocket>,
    subscription_attempt: &Cell<f64>,
    build_params: impl FnOnce() -> F,
) -> Result<(), ExchangeError>
where
    F: Future<Output = Result<JsJsonValue, ExchangeError>>,
{
    if socket.is_disposed() {
        return Ok(());
    }
    // ts-bug: the source Number counter can stop advancing beyond 2^53.
    let attempt = subscription_attempt.get() + 1.0;
    subscription_attempt.set(attempt);
    // No borrow guard crosses the await. Cancellation retains the updated
    // attempt; it is not an in-flight latch and needs no reset on Drop.
    // Concrete WsProtocol composition must use the core-owned open task.
    let result = async {
        let params = build_params().await?;
        if socket.is_disposed() || attempt != subscription_attempt.get() {
            return Ok(());
        }
        let frame = JsJsonValue::Object(vec![
            (
                "id".into(),
                JsJsonValue::String(crate::spot_protocol::BINANCE_SPOT_USER_DATA_REQUEST_ID.into()),
            ),
            (
                "method".into(),
                JsJsonValue::String("userDataStream.subscribe.signature".into()),
            ),
            ("params".into(), params),
        ]);
        let encoded = js_json_stringify(&frame)
            .map_err(|error| {
                let mut out = ExchangeError::new(error.message);
                out.name = error.name.into();
                out
            })?
            .ok_or_else(|| ExchangeError::new("Subscription frame serialized as undefined"))?;
        socket.send_with_log(&encoded)
    }
    .await;
    if let Err(error) = result {
        socket.emit_and_log_error(
            if error.message.is_empty() {
                "Binance Spot user-data subscription signing failed"
            } else {
                &error.message
            },
            None,
            None,
        )?;
        // Source catches rejected stale/disposed attempts too: they still close.
        socket.force_close();
    }
    Ok(())
}
