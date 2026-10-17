//! Public heartbeat operations over the core-owned socket.
use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
#[cfg(test)]
use safe_cex_num::{TableHit, js_table_lookup, js_truthy};
#[cfg(test)]
use safe_cex_transport::how::json_parse;
#[cfg(test)]
use serde_json::Value;
use std::rc::Rc;

// ts: handleTickersStreamEvents
pub fn handle_tickers_stream_events(
    store: &dyn safe_cex_store::Store,
    data: &serde_json::Value,
) -> Result<(), ExchangeError> {
    update_public_tickers(
        store,
        data,
        &[
            ("last", "close"),
            ("quoteVolume", "amount"),
            ("volume", "volume"),
        ],
    )
}

// ts: handleBBOStreamEvents
pub fn handle_bbo_stream_events(
    store: &dyn safe_cex_store::Store,
    data: &serde_json::Value,
) -> Result<(), ExchangeError> {
    update_public_tickers(store, data, &[("bid", "bid"), ("ask", "ask")])
}

// ts: handleMarkPricesStreamEvents
pub fn handle_mark_prices_stream_events(
    store: &dyn safe_cex_store::Store,
    data: &serde_json::Value,
) -> Result<(), ExchangeError> {
    update_public_tickers(store, data, &[("mark", "price")])
}

fn update_public_tickers(
    store: &dyn safe_cex_store::Store,
    data: &serde_json::Value,
    fields: &[(&str, &str)],
) -> Result<(), ExchangeError> {
    use safe_cex_store::{StoreMutationError, model_patch::ModelPatch};
    let rows = data.as_array().ok_or_else(|| {
        type_error(if data.is_null() {
            "Cannot read properties of null (reading 'forEach')"
        } else {
            "data.forEach is not a function"
        })
    })?;
    let mut refusals = Vec::new();
    'rows: for row in rows {
        if row.is_null() {
            return Err(type_error(
                "Cannot read properties of null (reading 'symbol')",
            ));
        }
        let symbol = match row.get("symbol") {
            Some(serde_json::Value::String(symbol)) => symbol,
            Some(serde_json::Value::Null) => {
                return Err(type_error(
                    "Cannot read properties of null (reading 'startsWith')",
                ));
            }
            None => {
                return Err(type_error(
                    "Cannot read properties of undefined (reading 'startsWith')",
                ));
            }
            _ => return Err(type_error("row.symbol.startsWith is not a function")),
        };
        if !symbol.starts_with("PERP_") {
            continue;
        }
        let symbol = crate::utils::normalize_symbol(symbol);
        // Source uppercasing prevents inherited-member spellings. Drop the
        // read borrow before the mutation or a synchronous callback can fail.
        let id = store
            .tickers_by_symbol()
            .get(&symbol)
            .map(|ticker| ticker.id.clone());
        let Some(id) = id else {
            continue;
        };
        let mut patch = ModelPatch::default();
        for (target, source) in fields {
            // Preserve later source/store errors ahead of typed row reports.
            let Some(value) = row.get(*source).filter(|v| v.is_number()) else {
                let value = row
                    .get(*source)
                    .map_or_else(|| "undefined".into(), serde_json::Value::to_string);
                refusals.push(format!(
                    "Refused WOO ticker row symbol={symbol:?}: {target}={value}: expected number"
                ));
                continue 'rows;
            };
            let number = safe_cex_num::numeric_ingress::js_value_as_f64(value)
                .ok_or_else(|| ExchangeError::new("numeric ticker value unavailable"))?;
            patch.set_number(*target, number);
        }
        // Preserve the typed mutation failure until core recovers its cause.
        let updated: Result<(), StoreMutationError> = store.update_ticker(&id, &patch);
        updated.map_err(|error: StoreMutationError| ExchangeError::from(error))?;
    }
    if refusals.is_empty() {
        Ok(())
    } else {
        Err(ExchangeError::new(refusals.join("; ")))
    }
}

fn type_error(message: &str) -> ExchangeError {
    let mut error = ExchangeError::new(message);
    error.name = "TypeError".into();
    error
}

/// Sends the legacy heartbeat after recording the monotonic ping time.
pub fn ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    socket.send(r#"{"event":"ping"}"#)
}

// ts: handlePingEvent
/// Replies through core send, including its disposed and closed-socket guards.
pub fn handle_ping_event(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    socket.send(r#"{"event":"pong"}"#)
}

#[cfg(test)]
pub(crate) fn on_message(socket: &Rc<BaseWebSocket>, data: &str) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received()?;
    let message = json_parse(data);
    if !js_truthy(Some(&message)) {
        return Ok(());
    }
    let key = if let Some(event) = message.get("event").and_then(Value::as_str) {
        if event != "ping" && event != "pong" {
            return Ok(());
        }
        event
    } else {
        message.get("topic").and_then(Value::as_str).unwrap_or("")
    };
    // Own keys shadow prototype members, including an explicitly empty list.
    if socket.message_handlers.borrow().contains_key(key) {
        return socket.dispatch_message(key, &message);
    }
    if let TableHit::Inherited(member) = js_table_lookup::<&str, ()>(&[], key)
        && member.length().is_some_and(|length| length != 0)
    {
        // ts-bug: function arity passes handlers?.length before for-of throws.
        // Core preserves inherited-handler reads but refuses global prototype
        // stats writes. Propagate its immediate source error before iteration.
        socket.record_message_handler_stats(key)?;
        let mut error = ExchangeError::new("handlers is not iterable");
        error.name = "TypeError".into();
        return Err(error);
    }
    Ok(())
}

#[cfg(test)]
#[path = "../tests/support/public_dispatch.rs"]
mod tests;

#[cfg(test)]
#[path = "../tests/support/public_writes.rs"]
mod write_tests;
