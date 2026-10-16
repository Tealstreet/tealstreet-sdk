use safe_cex_exchange::{runtime::ExchangeError, websocket::BaseWebSocket};
use serde_json::Value;
use std::{cell::RefCell, rc::Rc};

pub type Failure = Rc<ExchangeError>;
/// Decoded MessageEvent.data referent; None represents an undefined data field.
/// Captured-value contract: primitive cells stay immutable; object/array cells
/// retain their value/kind and identity. Callbacks may mutate their properties or
/// elements, but must not replace the captured cell's whole value. This matches
/// JavaScript's by-value primitive capture and retained object references.
/// Executable accessors are represented by the configured read_data boundary.
pub type MessageData = Rc<RefCell<Value>>;
pub type ReadData = Result<Option<MessageData>, Failure>;

// ts: BinanceMarketWebsocket.onMessage
pub fn on_futures_market_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    dispatch: impl FnOnce(Option<MessageData>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let data = read_data()?;
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received().map_err(Rc::new)?;
    dispatch(data)
}

// ts: BinancePublicWebsocket.onMessage
pub fn on_futures_public_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    dispatch: impl FnOnce(Option<MessageData>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let data = read_data()?;
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received().map_err(Rc::new)?;
    dispatch(data)
}

// ts: BinancePMMarketWebsocket.onMessage
pub fn on_pm_market_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    dispatch: impl FnOnce(Option<MessageData>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let data = read_data()?;
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received().map_err(Rc::new)?;
    dispatch(data)
}

// ts: BinancePMPublicWebsocket.onMessage
pub fn on_pm_public_message(
    socket: &Rc<BaseWebSocket>,
    read_data: impl FnOnce() -> ReadData,
    dispatch: impl FnOnce(Option<MessageData>) -> Result<(), Failure>,
) -> Result<(), Failure> {
    let data = read_data()?;
    if socket.is_disposed() {
        return Ok(());
    }
    socket.track_last_received().map_err(Rc::new)?;
    dispatch(data)
}
