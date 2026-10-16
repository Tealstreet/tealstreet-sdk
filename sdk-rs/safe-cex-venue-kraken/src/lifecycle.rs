//! Venue lifecycle bodies with host-bound socket and live Map operations.
use safe_cex_exchange::runtime::ExchangeError;
use std::{any::Any, cell::RefCell, rc::Rc};

pub type SocketCallback = Rc<dyn Fn(&dyn Any) -> Result<(), ExchangeError>>;

pub trait ConnectionSocket {
    fn ready_state(&self) -> Result<f64, ExchangeError>;
    fn close(&self) -> Result<(), ExchangeError>;
    fn add_event_listener(
        &self,
        event: &str,
        callback: SocketCallback,
    ) -> Result<(), ExchangeError>;
}

pub trait ConnectionHost {
    type Socket: ConnectionSocket + ?Sized;
    fn is_disposed(&self) -> bool;
    fn testnet(&self) -> bool;
    fn socket(&self) -> &RefCell<Option<Rc<Self::Socket>>>;
    fn create_websocket(&self, url: &str) -> Result<Rc<Self::Socket>, ExchangeError>;
    fn on_open(&self) -> SocketCallback;
    fn on_message(&self) -> SocketCallback;
    fn on_close(&self) -> Result<(), ExchangeError>;
}

fn current<H: ConnectionHost + ?Sized>(
    host: &H,
    member: &str,
) -> Result<Rc<H::Socket>, ExchangeError> {
    host.socket().borrow().clone().ok_or_else(|| {
        let mut error = ExchangeError::new(format!(
            "Cannot read properties of undefined (reading '{member}')"
        ));
        error.name = "TypeError".into();
        error
    })
}

fn close_current<H: ConnectionHost + ?Sized>(host: &H) -> Result<(), ExchangeError> {
    let present = host.socket().borrow().is_some();
    if present && current(host, "readyState")?.ready_state()? != 3.0 {
        current(host, "close")?.close()?;
    }
    Ok(())
}

fn install<H: ConnectionHost + ?Sized + 'static>(
    host: Rc<H>,
    url: &str,
) -> Result<(), ExchangeError> {
    let socket = host.create_websocket(url)?;
    *host.socket().borrow_mut() = Some(socket);
    let close_host = host.clone();
    let handle_close: SocketCallback = Rc::new(move |_| close_host.on_close());
    let error_host = host.clone();
    let error_close = handle_close.clone();
    let handle_error: SocketCallback = Rc::new(move |event| {
        // Source catches readyState/close failures, then always invokes onClose.
        if close_current(error_host.as_ref()).is_err() {
            return error_close(event);
        }
        error_close(event)
    });
    current(host.as_ref(), "addEventListener")?.add_event_listener("open", host.on_open())?;
    current(host.as_ref(), "addEventListener")?.add_event_listener("message", host.on_message())?;
    current(host.as_ref(), "addEventListener")?.add_event_listener("close", handle_close)?;
    current(host.as_ref(), "addEventListener")?.add_event_listener("error", handle_error)?;
    Ok(())
}

fn public_url(testnet: bool) -> &'static str {
    let network = if testnet { "testnet" } else { "livenet" };
    crate::types::BASE_WS_URL
        .iter()
        .find(|(key, _)| *key == "public")
        .unwrap()
        .1
        .iter()
        .find(|(key, _)| *key == network)
        .unwrap()
        .1
}

fn spot_url(channel: &str) -> &'static str {
    crate::spot_types::BASE_WS_URL
        .iter()
        .find(|(key, _)| *key == channel)
        .unwrap()
        .1
        .iter()
        .find(|(key, _)| *key == "livenet")
        .unwrap()
        .1
}

pub fn futures_public_connect<H: ConnectionHost + ?Sized + 'static>(
    host: Rc<H>,
) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        let url = public_url(host.testnet());
        install(host, url)?;
    }
    Ok(())
}

pub fn futures_private_connect<H: ConnectionHost + ?Sized + 'static>(
    host: Rc<H>,
) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        let prefix = if host.testnet() { "demo-" } else { "" };
        let url = format!("wss://{prefix}futures.kraken.com/ws/v1");
        install(host, &url)?;
    }
    Ok(())
}

pub fn spot_public_connect<H: ConnectionHost + ?Sized + 'static>(
    host: Rc<H>,
) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        install(host, spot_url("public"))?;
    }
    Ok(())
}

pub fn spot_private_connect<H: ConnectionHost + ?Sized + 'static>(
    host: Rc<H>,
) -> Result<(), ExchangeError> {
    if !host.is_disposed() {
        install(host, spot_url("private"))?;
    }
    Ok(())
}

pub trait DisposalHost {
    type Timeout: ?Sized;
    type Key: ?Sized;
    type Timeouts: Iterator<Item = Result<Rc<Self::Timeout>, ExchangeError>>;
    type Keys: Iterator<Item = Result<Rc<Self::Key>, ExchangeError>>;
    fn polling_timeouts(&self) -> Result<Self::Timeouts, ExchangeError>;
    fn clear_timeout(&self, timeout: &Rc<Self::Timeout>) -> Result<(), ExchangeError>;
    fn active_keys(&self) -> Result<Self::Keys, ExchangeError>;
    fn set_inactive(&self, key: &Rc<Self::Key>) -> Result<(), ExchangeError>;
    fn clear_timeouts(&self) -> Result<(), ExchangeError>;
    fn clear_active(&self) -> Result<(), ExchangeError>;
    fn clear_candles(&self) -> Result<(), ExchangeError>;
    fn dispose_base(&self) -> Result<(), ExchangeError>;
}

pub fn futures_dispose<H: DisposalHost + ?Sized>(host: &H) -> Result<(), ExchangeError> {
    for timeout in host.polling_timeouts()? {
        host.clear_timeout(&timeout?)?;
    }
    for key in host.active_keys()? {
        host.set_inactive(&key?)?;
    }
    host.clear_timeouts()?;
    host.clear_active()?;
    host.clear_candles()?;
    host.dispose_base()
}
