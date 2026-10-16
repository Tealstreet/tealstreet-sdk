//! Private websocket session recovery and position-fetch scheduling.
use crate::{
    builder_workflow::{JsPromise, JsResult},
    private_commands::PrivateCommands,
    session_host::{SessionHost, optional, session_template},
};
use safe_cex_exchange::runtime::LogSeverity;
use std::{hash::Hash, rc::Rc};

/// Method hooks invoke eagerly; returned futures bind deferred JavaScript await reactions.
/// Plain values still yield when awaited; dropping reconnect registers no await reaction.
/// The scheduler retains the invoked operation independently of its returned handle.
pub trait PrivateLifecycleHost: SessionHost {
    type FillKey: Clone + Eq + Hash + 'static;
    fn state(&self) -> &PrivateCommands<Self::Value, Self::FillKey>;
    fn now(&self) -> JsResult<Self::Value, f64>;
    fn last_received(&self) -> f64;
    fn set_last_received(&self, value: f64);
    fn last_received_timer(&self) -> Self::Value;
    fn ping_sent(&self) -> bool;
    fn set_ping_sent(&self, value: bool);
    fn iso_date(&self, value: f64) -> JsResult<Self::Value>;
    fn log(&self, message: Self::Value, severity: LogSeverity) -> JsResult<Self::Value, ()>;
    fn clear_timeout(&self, timer: Self::Value) -> JsResult<Self::Value, ()>;
    fn ping(&self) -> JsResult<Self::Value, ()>;
    fn reconnect(&self) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn routing_host(&self) -> JsResult<Self::Value>;
    fn socket(&self) -> Self::Value;
    fn set_socket(&self, socket: Self::Value);
    fn create_socket(&self, host: Self::Value) -> JsResult<Self::Value>;
    fn listener(&self, name: &str) -> JsResult<Self::Value>;
    fn emit_error(&self, message: Self::Value) -> JsResult<Self::Value, ()>;
    fn reset_retry_budget(&self) -> JsResult<Self::Value, ()>;
    fn fail_closed(&self, reason: &'static str) -> JsResult<Self::Value, ()>;
    fn authenticate(&self) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn api_extras(&self) -> JsResult<Self::Value>;
    fn schedule(&self, callback: Rc<dyn Fn()>, delay: f64) -> JsResult<Self::Value>;
    fn fetch_positions(&self) -> JsResult<Self::Value, JsPromise<Self::Value>>;
    fn store(&self) -> JsResult<Self::Value>;
    fn sync_positions(&self, positions: Self::Value) -> JsResult<Self::Value, ()>;
}
/// ts: GrvtPrivateWebSocket.checkLastReceived.
pub fn check_last_received<H: PrivateLifecycleHost>(h: &H) -> JsResult<H::Value, ()> {
    let now = h.now()?;
    let diff = now - h.last_received();
    let iso = h.iso_date(h.last_received())?;
    if diff > 90000.0 {
        h.log(session_template!(h; h.string("[GRVT][PWS] No private stream messages for "), h.number(diff),
            h.string("ms (last at "), iso, h.string("); reconnecting."))?, LogSeverity::Warning)?;
        h.clear_timeout(h.last_received_timer())?;
        h.set_last_received(0.0);
        h.set_ping_sent(false);
        drop(h.reconnect()?);
    } else if diff > 30000.0 && !h.ping_sent() {
        h.log(session_template!(h; h.string("[GRVT][PWS] No private stream messages for "), h.number(diff),
            h.string("ms (last at "), iso, h.string("); sending ping."))?, LogSeverity::Info)?;
        if h.ping().is_ok() {
            h.set_ping_sent(true);
        }
    } else if diff <= 30000.0 {
        h.set_ping_sent(false);
    }
    Ok(())
}
/// ts: GrvtPrivateWebSocket.connectAndSubscribe.
pub fn connect_and_subscribe<H: PrivateLifecycleHost>(h: Rc<H>) -> JsResult<H::Value, ()> {
    if h.state().is_disposed.get() {
        return Ok(());
    }
    let host = h.routing_host()?;
    if !h.truthy(&host) {
        if !h.state().reported_missing_proxy_host.get() {
            h.state().reported_missing_proxy_host.set(true);
            h.emit_error(h.string("GRVT private stream needs a proxy transport (its session cookie cannot be set from a browser); the active transport resolves none."))?;
        }
        return Ok(());
    }
    h.set_socket(h.create_socket(host)?);
    for name in ["open", "message", "close"] {
        let socket = h.socket();
        let add = h.get(&socket, "addEventListener")?;
        let listener = h.listener(name)?;
        h.call(add, socket, vec![h.string(name), listener])?;
    }
    let socket = h.socket();
    let add = h.get(&socket, "addEventListener")?;
    let host = h.clone();
    let callback = h.function(Rc::new(move |event| {
        let message = optional(host.as_ref(), event, "message")?;
        let message = if host.truthy(&message) {
            message
        } else {
            host.string("Unknown error")
        };
        host.log(
            session_template!(host; host.string("WebSocket connection error: "), message)?,
            LogSeverity::Error,
        )?;
        Ok(host.undefined())
    }));
    h.call(add, socket, vec![h.string("error"), callback])?;
    Ok(())
}
/// ts: GrvtPrivateWebSocket#refreshSessionAndReconnect.
pub fn refresh_session_and_reconnect<H: PrivateLifecycleHost>(
    h: Rc<H>,
    reason: &'static str,
) -> JsResult<H::Value, ()> {
    if h.state().session_refresh_in_flight.get()
        || h.state().is_disposed.get()
        || h.state().authentication_failed.get()
    {
        return Ok(());
    }
    h.reset_retry_budget()?;
    if h.state().session_refresh_attempts.get() >= 1.0 {
        return h.fail_closed(reason);
    }
    h.state().session_refresh_in_flight.set(true);
    h.state()
        .session_refresh_attempts
        .set(h.state().session_refresh_attempts.get() + 1.0);
    h.state().authenticated.set(false);
    let host = h.clone();
    drop(h.start(Box::pin(async move {
        let result: JsResult<H::Value, ()> = async {
            let refreshed = host.authenticate()?.await?;
            if !host.truthy(&refreshed) || !host.truthy(&optional(host.as_ref(), host.api_extras()?, "sessionCookie")?)
                || !host.truthy(&host.get(&host.api_extras()?, "accountId")?) {
                return Err(host.error(host.string("GRVT authentication did not return a complete session"))?);
            }
            if host.state().is_disposed.get() { return Ok(()); }
            host.log(session_template!(host; host.string("Refreshing GRVT private WebSocket session after "), host.string(reason))?, LogSeverity::Warning)?;
            host.reconnect()?.await?;
            Ok(())
        }.await;
        let result = if result.is_err() { host.fail_closed("session refresh failed") } else { result };
        host.state().session_refresh_in_flight.set(false);
        result.map(|()| host.undefined())
    })));
    Ok(())
}
/// ts: GrvtPrivateWebSocket#throttledFetchPositions.
pub fn throttled_fetch_positions<H: PrivateLifecycleHost>(h: Rc<H>) -> JsResult<H::Value, ()> {
    let now = h.now()?;
    if h.state().is_disposed.get() || h.state().position_fetch_scheduled.get() {
        return Ok(());
    }
    let elapsed = now - h.state().last_position_fetch_time.get();
    if elapsed >= h.state().position_fetch_throttle_ms.get() {
        drop(execute_position_fetch(h));
    } else {
        h.state().position_fetch_scheduled.set(true);
        let host = h.clone();
        h.schedule(
            Rc::new(move || {
                host.state().position_fetch_scheduled.set(false);
                if !host.state().is_disposed.get() && host.state().has_pending_position_events.get()
                {
                    drop(execute_position_fetch(host.clone()));
                }
            }),
            h.state().position_fetch_throttle_ms.get() - elapsed,
        )?;
    }
    Ok(())
}
/// ts: GrvtPrivateWebSocket#executePositionFetch.
pub fn execute_position_fetch<H: PrivateLifecycleHost>(h: Rc<H>) -> JsPromise<H::Value> {
    let host = h.clone();
    h.start(Box::pin(async move {
        let result: JsResult<H::Value, ()> = async {
            host.state().last_position_fetch_time.set(host.now()?);
            host.state().has_pending_position_events.set(false);
            let positions = host.fetch_positions()?.await?;
            if host.state().is_disposed.get() {
                return Ok(());
            }
            let store = host.store()?;
            let update = host.get(&store, "update")?;
            let loaded = host.object(vec![]);
            host.spread(&loaded, host.get(&host.store()?, "loaded")?)?;
            host.define(&loaded, "positions", host.boolean(true))?;
            let changes = host.object(vec![("positions", positions.clone()), ("loaded", loaded)]);
            host.call(update, store, vec![changes])?;
            host.sync_positions(positions)?;
            Ok(())
        }
        .await;
        // Source intentionally suppresses all fetch/update/sync failures.
        drop(result);
        Ok(host.undefined())
    }))
}
