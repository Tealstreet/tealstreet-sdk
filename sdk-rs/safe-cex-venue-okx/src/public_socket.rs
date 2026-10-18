//! Public OKX frame dispatch and sequence recovery, using the core handler registry.
use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use safe_cex_num::{JsCoercionError, JsJsonValue, js_json_stringify, js_string};
use safe_cex_transport::socket::SocketFrame;
use serde_json::{Value, json};
use std::{
    cell::RefCell,
    collections::{HashMap, HashSet},
    rc::Rc,
};

fn coercion(error: JsCoercionError) -> ExchangeError {
    let mut error_result = ExchangeError::new(error.message);
    error_result.name = error.name.into();
    error_result
}

fn sequence(value: Option<&Value>) -> Result<String, ExchangeError> {
    match value.filter(|value| !value.is_null()) {
        None => Ok(String::new()),
        Some(value) => js_string(Some(value)).map_err(coercion),
    }
}

pub fn accept_order_book_sequence(
    handler: &str,
    data: &Value,
    sequences: &RefCell<HashMap<String, String>>,
    recovering: &RefCell<HashSet<String>>,
    recover: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<bool, ExchangeError> {
    let update = data.get("data").and_then(|data| {
        if data.is_object() {
            data.get("0")
        } else {
            data.get(0)
        }
    });
    let seq_id = sequence(update.and_then(|update| update.get("seqId")))?;
    if data.get("action").and_then(Value::as_str) == Some("snapshot") && !seq_id.is_empty() {
        sequences.borrow_mut().insert(handler.into(), seq_id);
        recovering.borrow_mut().remove(handler);
        return Ok(true);
    }
    if recovering.borrow().contains(handler) {
        return Ok(false);
    }
    let previous = sequence(update.and_then(|update| update.get("prevSeqId")))?;
    let expected = sequences.borrow().get(handler).cloned();
    if seq_id.is_empty()
        || previous.is_empty()
        || expected
            .as_deref()
            .is_none_or(|expected| expected.is_empty() || expected != previous)
    {
        // No state borrow survives this callback; send may synchronously reenter dispatch.
        recover(handler)?;
        return Ok(false);
    }
    sequences.borrow_mut().insert(handler.into(), seq_id);
    Ok(true)
}

pub fn recover_order_book(
    socket: &Rc<BaseWebSocket>,
    handler: &str,
    sequences: &RefCell<HashMap<String, String>>,
    recovering: &RefCell<HashSet<String>>,
    topics: &RefCell<HashMap<String, Vec<(String, String)>>>,
) -> Result<(), ExchangeError> {
    sequences.borrow_mut().remove(handler);
    recovering.borrow_mut().insert(handler.into());
    let topic = topics
        .borrow()
        .get(handler)
        .and_then(|topics| topics.first())
        .cloned();
    let Some((channel, instrument)) = topic else {
        return Ok(());
    };
    if !socket.is_connected() || socket.is_disposed() {
        return Ok(());
    }
    for operation in ["unsubscribe", "subscribe"] {
        let payload = JsJsonValue::from(
            json!({"op":operation,"args":[{"channel":channel,"instId":instrument}]}),
        );
        let text = js_json_stringify(&payload)
            .map_err(coercion)?
            .unwrap_or_default();
        socket.send(&text)?;
    }
    Ok(())
}

pub fn dispatch_message_handler(
    socket: &Rc<BaseWebSocket>,
    handler: &str,
    data: &Value,
    sequences: &RefCell<HashMap<String, String>>,
    recovering: &RefCell<HashSet<String>>,
    recover: &dyn Fn(&str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let callbacks = socket.message_handlers.borrow().get(handler).cloned();
    if let Some(callbacks) = &callbacks {
        // Own lists, including empty lists, shadow every inherited member.
        if callbacks.borrow().is_empty() {
            return Ok(());
        }
    } else {
        // ts-bug: a positive-arity Object.prototype function passes callbacks?.length.
        // The shared read preserves member identity; core owns statistics and errors.
        if let safe_cex_num::TableHit::Inherited(member) =
            safe_cex_num::js_table_lookup::<&str, ()>(&[], handler)
            && member.length().is_some_and(|length| length > 0)
        {
            socket.record_message_handler_stats(handler)?;
            let mut error = ExchangeError::new("callbacks is not iterable");
            error.name = "TypeError".into();
            return Err(error);
        }
        return Ok(());
    }
    if handler.starts_with("books.")
        && !accept_order_book_sequence(handler, data, sequences, recovering, recover)?
    {
        return Ok(());
    }
    socket.dispatch_message(handler, data)
}

pub fn on_message(
    socket: &Rc<BaseWebSocket>,
    frame: &SocketFrame,
    sequences: &RefCell<HashMap<String, String>>,
    recovering: &RefCell<HashSet<String>>,
    topics: &RefCell<HashMap<String, Vec<(String, String)>>>,
) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received()?;
    socket.direct_fallback.borrow_mut().on_message();
    let SocketFrame::Text(text) = frame else {
        return Ok(());
    };
    if text == "pong" {
        socket.handle_pong_event(5_000)?;
        return Ok(());
    }
    let data = safe_cex_transport::how::big_int_as_string_json_parse(text)
        .into_value()
        .map_err(|error| ExchangeError::new(error.to_string()))?;
    if data.is_null() || data.get("event").and_then(Value::as_str) == Some("subscribe") {
        return Ok(());
    }
    let Some(channel) = data
        .get("arg")
        .and_then(|arg| arg.get("channel"))
        .and_then(Value::as_str)
        .filter(|channel| !channel.is_empty())
    else {
        return Ok(());
    };
    let instrument = data
        .get("arg")
        .and_then(|arg| arg.get("instId"))
        .and_then(Value::as_str)
        .unwrap_or("");
    let recover =
        |handler: &str| recover_order_book(socket, handler, sequences, recovering, topics);
    if !instrument.is_empty() {
        dispatch_message_handler(
            socket,
            &format!("{channel}.{instrument}"),
            &data,
            sequences,
            recovering,
            &recover,
        )?;
    }
    dispatch_message_handler(socket, channel, &data, sequences, recovering, &recover)
}

fn ticker_update(data: &Value) -> Result<&Value, ExchangeError> {
    data.get("data")
        .and_then(Value::as_array)
        .and_then(|rows| rows.first())
        .filter(|row| !row.is_null())
        .ok_or_else(|| {
            let mut error = ExchangeError::new("ticker frame requires a first update record");
            error.name = "TypeError".into();
            error
        })
}

/// Adapted receiver: applies the first upstream update through the shared Store.
pub fn handle_open_interest_events(
    store: &dyn safe_cex_store::Store,
    data: &Value,
) -> Result<(), ExchangeError> {
    let update = ticker_update(data)?;
    let value = safe_cex_num::parse_float(&js_string(update.get("oiCcy")).map_err(coercion)?);
    let id = js_string(update.get("instId")).map_err(coercion)?;
    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
    patch.set_number("openInterest", value);
    store
        .update_ticker(&id, &patch)
        .map_err(|error: safe_cex_store::StoreMutationError| ExchangeError::from(error))
}

/// Adapted receiver: preserves funding-rate/time evaluation before the Store write.
pub fn handle_funding_rate_events(
    store: &dyn safe_cex_store::Store,
    data: &Value,
) -> Result<(), ExchangeError> {
    let update = ticker_update(data)?;
    let rate = safe_cex_num::parse_float(&js_string(update.get("fundingRate")).map_err(coercion)?);
    let time = safe_cex_num::parse_float(&js_string(update.get("fundingTime")).map_err(coercion)?);
    let id = js_string(update.get("instId")).map_err(coercion)?;
    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
    patch.set_number("fundingRate", rate);
    patch.set_number("fundingTime", time);
    store
        .update_ticker(&id, &patch)
        .map_err(|error: safe_cex_store::StoreMutationError| ExchangeError::from(error))
}

/// Adapted parent callback runs before parsing or writing the mark price.
pub fn handle_mark_price_events<M>(
    store: &dyn safe_cex_store::Store,
    data: &Value,
    mark_loaded: M,
) -> Result<(), ExchangeError>
where
    M: Fn(Option<&Value>) -> Result<(), ExchangeError>,
{
    let update = ticker_update(data)?;
    mark_loaded(update.get("instId"))?;
    let mark = safe_cex_num::parse_float(&js_string(update.get("markPx")).map_err(coercion)?);
    let id = js_string(update.get("instId")).map_err(coercion)?;
    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
    patch.set_number("mark", mark);
    store
        .update_ticker(&id, &patch)
        .map_err(|error: safe_cex_store::StoreMutationError| ExchangeError::from(error))
}

/// Adapted market selector returns borrowed IDs in the source market order.
pub fn handle_index_ticker_events<'a, G, L>(
    store: &dyn safe_cex_store::Store,
    data: &Value,
    get_market_ids: G,
    index_loaded: L,
) -> Result<(), ExchangeError>
where
    G: Fn(Option<&Value>) -> Result<Vec<&'a str>, ExchangeError>,
    L: Fn(&str) -> Result<(), ExchangeError>,
{
    let update = ticker_update(data)?;
    let price = safe_cex_num::parse_float(&js_string(update.get("idxPx")).map_err(coercion)?);
    let market_ids = get_market_ids(update.get("instId"))?;
    for id in market_ids {
        index_loaded(id)?;
        let mut patch = safe_cex_store::model_patch::ModelPatch::default();
        patch.set_number("index", price);
        store
            .update_ticker(id, &patch)
            .map_err(|error: safe_cex_store::StoreMutationError| ExchangeError::from(error))?;
    }
    Ok(())
}

/// Adapted ticker receiver keeps upstream volume units and shared USD rounding.
pub fn handle_ticker_events(
    store: &dyn safe_cex_store::Store,
    data: &Value,
) -> Result<(), ExchangeError> {
    let update = ticker_update(data)?;
    let open = safe_cex_num::parse_float(&js_string(update.get("open24h")).map_err(coercion)?);
    let last = safe_cex_num::parse_float(&js_string(update.get("last")).map_err(coercion)?);
    let percentage = safe_cex_num::round_usd(((last - open) / open) * 100.0);
    let bid = safe_cex_num::parse_float(&js_string(update.get("bidPx")).map_err(coercion)?);
    let ask = safe_cex_num::parse_float(&js_string(update.get("askPx")).map_err(coercion)?);
    let volume = safe_cex_num::parse_float(&js_string(update.get("volCcy24h")).map_err(coercion)?);
    let quote_volume =
        safe_cex_num::parse_float(&js_string(update.get("vol24h")).map_err(coercion)?);
    let id = js_string(update.get("instId")).map_err(coercion)?;
    let mut patch = safe_cex_store::model_patch::ModelPatch::default();
    patch.set_number("bid", bid);
    patch.set_number("ask", ask);
    patch.set_number("last", last);
    patch.set_number("percentage", percentage);
    patch.set_number("volume", volume);
    patch.set_number("quoteVolume", quote_volume);
    store
        .update_ticker(&id, &patch)
        .map_err(|error: safe_cex_store::StoreMutationError| ExchangeError::from(error))
}

/// Adapted topics receiver: own JSON members retain insertion order.
pub fn subscribe(
    socket: &Rc<BaseWebSocket>,
    topics: &serde_json::Map<String, Value>,
) -> Result<(), ExchangeError> {
    let mut flattened = Vec::new();
    for key in safe_cex_num::js_own_keys(topics.keys().map(String::as_str)) {
        match &topics[key] {
            Value::Array(items) => flattened.extend(items.iter().cloned()),
            item => flattened.push(item.clone()),
        }
    }
    for batch in flattened.chunks(100) {
        let payload = JsJsonValue::Object(vec![
            ("op".into(), JsJsonValue::String("subscribe".into())),
            (
                "args".into(),
                JsJsonValue::Array(batch.iter().cloned().map(JsJsonValue::from).collect()),
            ),
        ]);
        let text = js_json_stringify(&payload)
            .map_err(coercion)?
            .unwrap_or_default();
        socket.send(&text)?;
    }
    Ok(())
}

/// Record the heartbeat timestamp before the source's literal wire send.
pub fn ping(socket: &Rc<BaseWebSocket>) -> Result<(), ExchangeError> {
    if socket.is_disposed() {
        return Ok(());
    }
    socket.mark_ping_sent();
    socket.send("ping")
}
