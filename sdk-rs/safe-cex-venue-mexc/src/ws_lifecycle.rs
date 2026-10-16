//! Exact venue heartbeat/lifecycle bodies over injected socket and timer hosts.
//! Timer handles/callbacks are host-owned; the full websocket constructors and
//! exchange composition remain separate manifest rows.
use std::{cell::RefCell, rc::Rc};

/// Runtime boundaries of the source setInterval/clearInterval calls and the
/// receiver's ping/state getters. The timer host retains scheduled callbacks
/// independently of the call that registers them, until cancellation.
pub trait HeartbeatHost: 'static {
    type Handle: 'static;
    type Error: 'static;
    fn is_disposed(&self) -> bool;
    fn is_connected(&self) -> bool;
    fn clear_interval(&self, handle: Option<Rc<Self::Handle>>) -> Result<(), Self::Error>;
    fn set_interval(
        &self,
        period_ms: u64,
        callback: Rc<dyn Fn() -> Result<(), Self::Error>>,
    ) -> Result<Rc<Self::Handle>, Self::Error>;
    fn ping(&self) -> Result<(), Self::Error>;
}

/// Source MexcPublicWebsocket#stopHeartbeat, including clearInterval(undefined).
pub fn stop_public_heartbeat<H: HeartbeatHost>(
    host: &H,
    slot: &RefCell<Option<Rc<H::Handle>>>,
) -> Result<(), H::Error> {
    let handle = slot.borrow().clone();
    host.clear_interval(handle)?;
    *slot.borrow_mut() = None;
    Ok(())
}

/// Source MexcPrivateWebsocket#stopHeartbeat.
pub fn stop_private_heartbeat<H: HeartbeatHost>(
    host: &H,
    slot: &RefCell<Option<Rc<H::Handle>>>,
) -> Result<(), H::Error> {
    let handle = slot.borrow().clone();
    host.clear_interval(handle)?;
    *slot.borrow_mut() = None;
    Ok(())
}

/// Source MexcPublicWebsocket#startHeartbeat; registration is unconditional.
pub fn start_public_heartbeat<H: HeartbeatHost>(
    host: Rc<H>,
    slot: &RefCell<Option<Rc<H::Handle>>>,
) -> Result<(), H::Error> {
    stop_public_heartbeat(host.as_ref(), slot)?;
    host.ping()?;
    let ticking = host.clone();
    let callback = Rc::new(move || {
        if !ticking.is_disposed() && ticking.is_connected() {
            ticking.ping()?;
        }
        Ok(())
    });
    let handle = host.set_interval(crate::types::MEXC_WS_HEARTBEAT_INTERVAL_MS, callback)?;
    *slot.borrow_mut() = Some(handle);
    Ok(())
}

/// Source MexcPrivateWebsocket#startHeartbeat.
pub fn start_private_heartbeat<H: HeartbeatHost>(
    host: Rc<H>,
    slot: &RefCell<Option<Rc<H::Handle>>>,
) -> Result<(), H::Error> {
    stop_private_heartbeat(host.as_ref(), slot)?;
    host.ping()?;
    let ticking = host.clone();
    let callback = Rc::new(move || {
        if !ticking.is_disposed() && ticking.is_connected() {
            ticking.ping()?;
        }
        Ok(())
    });
    let handle = host.set_interval(crate::types::MEXC_WS_HEARTBEAT_INTERVAL_MS, callback)?;
    *slot.borrow_mut() = Some(handle);
    Ok(())
}

/// Source MexcPublicWebsocket.onOpen: guard once, then preserve call order.
pub fn public_on_open<E>(
    is_disposed: bool,
    start: impl FnOnce() -> Result<(), E>,
    reconcile: impl FnOnce() -> Result<(), E>,
    resubscribe: impl FnOnce() -> Result<(), E>,
    reconnected: impl FnOnce() -> Result<(), E>,
) -> Result<(), E> {
    if is_disposed {
        return Ok(());
    }
    start()?;
    reconcile()?;
    resubscribe()?;
    reconnected()?;
    Ok(())
}

/// Source MexcPublicWebsocket.ping; performance.now is assigned before sending.
pub fn ping_public<E>(
    is_disposed: bool,
    ping_at: &RefCell<f64>,
    performance_now: impl FnOnce() -> Result<f64, E>,
    send_with_diagnostics: impl FnOnce(&str) -> Result<(), E>,
) -> Result<(), E> {
    if !is_disposed {
        let now = performance_now()?;
        *ping_at.borrow_mut() = now;
        send_with_diagnostics("{\"method\":\"ping\"}")?;
    }
    Ok(())
}

/// Source MexcPublicWebsocket#handleClose; event is forwarded unchanged.
pub fn handle_public_close<T, E, R>(
    stop: impl FnOnce() -> Result<(), E>,
    on_close: impl FnOnce(T) -> Result<R, E>,
    event: T,
) -> Result<(), E> {
    stop()?;
    on_close(event)?;
    Ok(())
}

/// Source MexcPrivateWebsocket#handleClose.
pub fn handle_private_close<T, E, R>(
    stop: impl FnOnce() -> Result<(), E>,
    on_close: impl FnOnce(T) -> Result<R, E>,
    event: T,
) -> Result<(), E> {
    stop()?;
    on_close(event)?;
    Ok(())
}

/// Source MexcPublicWebsocket.dispose; remove the original handler before super.
pub fn dispose_public<T, E, R>(
    stop: impl FnOnce() -> Result<(), E>,
    off: impl FnOnce(&str, T) -> Result<(), E>,
    handler: impl FnOnce() -> T,
    parent_dispose: impl FnOnce() -> Result<R, E>,
) -> Result<(), E> {
    stop()?;
    off("activeTickerSymbols:changed", handler())?;
    parent_dispose()?;
    Ok(())
}

/// Source MexcPrivateWebsocket.dispose; refresh handle is read after heartbeat
/// cleanup and its property is retained even after clearTimeout.
pub fn dispose_private<T, E, R>(
    stop: impl FnOnce() -> Result<(), E>,
    refresh: &RefCell<Option<Rc<T>>>,
    clear_timeout: impl FnOnce(Option<Rc<T>>) -> Result<(), E>,
    parent_dispose: impl FnOnce() -> Result<R, E>,
) -> Result<(), E> {
    stop()?;
    let handle = refresh.borrow().clone();
    clear_timeout(handle)?;
    parent_dispose()?;
    Ok(())
}
