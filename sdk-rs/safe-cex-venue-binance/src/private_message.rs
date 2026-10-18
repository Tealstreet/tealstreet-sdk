use crate::public_message::{Failure, MessageData, ReadData};
use safe_cex_exchange::websocket::BaseWebSocket;
use safe_cex_num::js_string;
use safe_cex_transport::how::{big_int_as_string_json_parse, json_parse};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

/// Current bound class methods. Diagnostic errors, callback mutations and original
/// callback errors remain visible. The concrete class supplies these methods.
pub trait PrivateMessageMethods {
    fn diagnostic(&mut self, data: Option<&MessageData>) -> Result<(), Failure>;
    fn account(&mut self, events: Vec<MessageData>) -> Result<(), Failure>;
    fn order(&mut self, events: Vec<MessageData>) -> Result<(), Failure>;
    fn algo_order(&mut self, events: Vec<MessageData>) -> Result<(), Failure>;
    fn conditional_order(&mut self, events: Vec<MessageData>) -> Result<(), Failure>;
    fn pong(&mut self) -> Result<(), Failure>;
}

// ts: BinancePrivateWebsocket.onMessage
pub fn on_futures_private_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    methods: &mut impl PrivateMessageMethods,
) -> Result<(), Failure> {
    let data = read_data()?;
    if !socket.is_disposed() {
        socket.track_last_received().map_err(Rc::new)?;
        methods.diagnostic(data.as_ref())?;
        let json = match data.as_ref() {
            Some(data) if data.borrow().is_string() => Some(Rc::new(RefCell::new(
                big_int_as_string_json_parse(data.borrow().as_str().unwrap())
                    .into_value()
                    .map_err(|error| {
                        Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
                            error.to_string(),
                        ))
                    })?,
            ))),
            // The actual bigint parser returns non-string input unchanged.
            _ => data,
        };
        if event_is(&json, "ACCOUNT_UPDATE") {
            socket
                .record_message_handler_stats("position")
                .map_err(Rc::new)?;
            methods.account(vec![json.as_ref().unwrap().clone()])?;
        }
        if event_is(&json, "ORDER_TRADE_UPDATE") {
            socket
                .record_message_handler_stats("order")
                .map_err(Rc::new)?;
            methods.order(vec![json.as_ref().unwrap().clone()])?;
        }
        if event_is(&json, "ALGO_UPDATE") {
            socket
                .record_message_handler_stats("algo_order")
                .map_err(Rc::new)?;
            methods.algo_order(vec![json.as_ref().unwrap().clone()])?;
        }
        if id_is_pong(&json) {
            methods.pong()?;
        }
    }
    Ok(())
}

// ts: BinancePMPrivateWebsocket.onMessage
pub fn on_pm_private_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    methods: &mut impl PrivateMessageMethods,
) -> Result<(), Failure> {
    let data = read_data()?;
    if !socket.is_disposed() {
        socket.track_last_received().map_err(Rc::new)?;
        methods.diagnostic(data.as_ref())?;
        // The source jsonParse catches JSON.parse input-coercion errors too.
        let parsed = js_string(data.as_ref().map(|value| value.borrow()).as_deref())
            .map_or(Ok(Value::Null), |text| json_parse(&text).into_value())
            .map_err(|error| {
                Rc::new(safe_cex_exchange::runtime::ExchangeError::new(
                    error.to_string(),
                ))
            })?;
        let json = Some(Rc::new(RefCell::new(parsed)));
        if event_is(&json, "ACCOUNT_UPDATE") {
            socket
                .record_message_handler_stats("position")
                .map_err(Rc::new)?;
            methods.account(vec![json.as_ref().unwrap().clone()])?;
        }
        if event_is(&json, "ORDER_TRADE_UPDATE") {
            socket
                .record_message_handler_stats("order")
                .map_err(Rc::new)?;
            methods.order(vec![json.as_ref().unwrap().clone()])?;
        }
        if event_is(&json, "CONDITIONAL_ORDER_TRADE_UPDATE") {
            socket
                .record_message_handler_stats("conditional-order")
                .map_err(Rc::new)?;
            methods.conditional_order(vec![json.as_ref().unwrap().clone()])?;
        }
        if id_is_pong(&json) {
            methods.pong()?;
        }
    }
    Ok(())
}

fn event_is(json: &Option<MessageData>, event: &str) -> bool {
    json.as_ref()
        .is_some_and(|json| json.borrow().get("e").and_then(Value::as_str) == Some(event))
}
fn id_is_pong(json: &Option<MessageData>) -> bool {
    json.as_ref()
        .is_some_and(|json| json.borrow().get("id").and_then(Value::as_f64) == Some(42.0))
}
