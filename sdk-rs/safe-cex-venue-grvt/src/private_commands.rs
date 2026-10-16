//! GRVT private websocket commands against bound clock, logger, send and disposal hooks.
use crate::public_subscriptions::NumberKey;
use indexmap::{IndexMap, IndexSet};
use safe_cex_exchange::runtime::{ExchangeError, LogSeverity};
use safe_cex_num::{JsJsonValue, js_json_stringify};
use std::{
    cell::{Cell, RefCell},
    hash::Hash,
    rc::Rc,
};

pub type StreamFeed = Rc<RefCell<Vec<String>>>;
#[derive(Clone, Debug)]
pub struct LegacySubscription {
    pub request_id: f64,
    pub method: crate::types::GrvtSubscriptionMethod,
    pub stream: String,
    pub feed: StreamFeed,
    pub is_full: bool,
}
#[derive(Clone, Debug)]
pub struct ProxyAuthRequest {
    pub kind: &'static str,
    pub session_cookie: String,
    pub account_id: Option<String>,
    pub testnet: bool,
}
fn object(fields: Vec<(&str, JsJsonValue)>) -> Result<String, ExchangeError> {
    js_json_stringify(&JsJsonValue::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    ))
    .map_err(|cause| ExchangeError::new(cause.message))
    .map(|wire| wire.expect("command object"))
}
impl LegacySubscription {
    fn wire(&self) -> Result<String, ExchangeError> {
        let method = match self.method {
            crate::types::GrvtSubscriptionMethod::Subscribe => "subscribe",
            crate::types::GrvtSubscriptionMethod::Unsubscribe => "unsubscribe",
        };
        object(vec![
            ("request_id", JsJsonValue::Number(self.request_id)),
            ("method", JsJsonValue::String(method.into())),
            ("stream", JsJsonValue::String(self.stream.clone())),
            (
                "feed",
                JsJsonValue::Array(
                    self.feed
                        .borrow()
                        .iter()
                        .cloned()
                        .map(JsJsonValue::String)
                        .collect(),
                ),
            ),
            ("is_full", JsJsonValue::Bool(self.is_full)),
        ])
    }
}
impl ProxyAuthRequest {
    fn wire(&self) -> Result<String, ExchangeError> {
        object(vec![
            ("type", JsJsonValue::String(self.kind.into())),
            (
                "session_cookie",
                JsJsonValue::String(self.session_cookie.clone()),
            ),
            (
                "account_id",
                self.account_id
                    .as_ref()
                    .map(|id| JsJsonValue::String(id.clone()))
                    .unwrap_or(JsJsonValue::Undefined),
            ),
            ("testnet", JsJsonValue::Bool(self.testnet)),
        ])
    }
}

pub struct PrivateCommands<T, F = String> {
    pub is_disposed: Cell<bool>,
    pub authenticated: Cell<bool>,
    pub authentication_failed: Cell<bool>,
    pub private_stream_healthy_at: Cell<Option<f64>>,
    pub session_refresh_attempts: Cell<f64>,
    pub request_id: Cell<f64>,
    pub ping_at: Cell<f64>,
    pub subscriptions: RefCell<IndexMap<NumberKey, LegacySubscription>>,
    pub processed_fill_ids: RefCell<IndexSet<F>>,
    pub orders_throttle_timeout: RefCell<Option<T>>,
    pub has_pending_position_events: Cell<bool>,
    pub reported_missing_proxy_host: Cell<bool>,
    pub session_refresh_in_flight: Cell<bool>,
    pub last_position_fetch_time: Cell<f64>,
    pub position_fetch_throttle_ms: Cell<f64>,
    pub position_fetch_scheduled: Cell<bool>,
}
impl<T, F> Default for PrivateCommands<T, F> {
    fn default() -> Self {
        Self {
            is_disposed: Cell::new(false),
            authenticated: Cell::new(false),
            authentication_failed: Cell::new(false),
            private_stream_healthy_at: Cell::new(None),
            session_refresh_attempts: Cell::new(0.0),
            request_id: Cell::new(1.0),
            ping_at: Cell::new(0.0),
            subscriptions: RefCell::new(IndexMap::new()),
            processed_fill_ids: RefCell::new(IndexSet::new()),
            orders_throttle_timeout: RefCell::new(None),
            has_pending_position_events: Cell::new(false),
            reported_missing_proxy_host: Cell::new(false),
            session_refresh_in_flight: Cell::new(false),
            last_position_fetch_time: Cell::new(0.0),
            position_fetch_throttle_ms: Cell::new(500.0),
            position_fetch_scheduled: Cell::new(false),
        }
    }
}
#[derive(Debug, Clone, Copy)]
pub enum AuthenticationFailureReason {
    ProxyRejected,
    GrvtError1000,
    RefreshFailed,
}
impl AuthenticationFailureReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ProxyRejected => "proxy authentication rejected",
            Self::GrvtError1000 => "GRVT error 1000",
            Self::RefreshFailed => "session refresh failed",
        }
    }
}

/// ts: GrvtPrivateWebSocket.getWatchdogStaleThresholdMs.
pub fn get_watchdog_stale_threshold_ms() -> f64 {
    90000.0
}

/// ts: GrvtPrivateWebSocket#authenticate. The account slot retains undefined until wire projection.
pub fn authenticate(
    session_cookie: impl FnOnce() -> Option<String>,
    account_id: impl FnOnce() -> Option<String>,
    is_testnet: impl FnOnce() -> bool,
    mut log: impl FnMut(&str, Option<LogSeverity>) -> Result<(), ExchangeError>,
    mut send: impl FnMut(&ProxyAuthRequest, &str) -> Result<(), ExchangeError>,
) -> Result<(), ExchangeError> {
    let cookie = session_cookie()
        .filter(|cookie| !cookie.is_empty())
        .map(|cookie| format!("rm=true; gravity={cookie}"))
        .unwrap_or_default();
    let request = ProxyAuthRequest {
        kind: "auth",
        session_cookie: cookie,
        account_id: account_id(),
        testnet: is_testnet(),
    };
    log("Sending authentication to GRVT proxy", None)?;
    send(&request, &request.wire()?)
}

/// ts: GrvtPrivateWebSocket#subscribe. All streams receive the same live feed array.
pub fn subscribe(
    account_id: impl FnOnce() -> Option<String>,
    mut log: impl FnMut(&str, Option<LogSeverity>) -> Result<(), ExchangeError>,
    mut stream: impl FnMut(&str, StreamFeed) -> Result<f64, ExchangeError>,
) -> Result<(), ExchangeError> {
    let Some(account) = account_id().filter(|account| !account.is_empty()) else {
        log(
            "No trading account ID provided - skipping private WS subscriptions",
            Some(LogSeverity::Warning),
        )?;
        return Ok(());
    };
    let feed = Rc::new(RefCell::new(vec![account.clone()]));
    log(
        &format!("Subscribing to private streams for sub-account: {account}"),
        None,
    )?;
    stream("v1.order", feed.clone())?;
    stream("v1.position", feed.clone())?;
    stream("v1.fill", feed)?;
    Ok(())
}
impl<T, F: Eq + Hash> PrivateCommands<T, F> {
    /// ts: GrvtPrivateWebSocket.ping. Clock assignment precedes the send even if guards change.
    pub fn ping(
        &self,
        now: impl FnOnce() -> Result<f64, ExchangeError>,
        mut send: impl FnMut(&str) -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if !self.is_disposed.get() && !self.authentication_failed.get() {
            self.ping_at.set(now()?);
            send("ping")?;
        }
        Ok(())
    }
    /// ts: GrvtPrivateWebSocket.onOpen.
    pub fn on_open(
        &self,
        mut log: impl FnMut(&str, Option<LogSeverity>) -> Result<(), ExchangeError>,
        mut authenticate: impl FnMut() -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if !self.is_disposed.get() && !self.authentication_failed.get() {
            self.authenticated.set(false);
            self.private_stream_healthy_at.set(None);
            log("GRVT private WebSocket connected to proxy", None)?;
            authenticate()?;
        }
        Ok(())
    }
    /// ts: GrvtPrivateWebSocket#resetRetryBudgetAfterHealthySession.
    pub fn reset_retry_budget_after_healthy_session(
        &self,
        now: impl FnOnce() -> Result<f64, ExchangeError>,
    ) -> Result<(), ExchangeError> {
        if self.private_stream_healthy_at.get().is_some() {
            let now = now()?;
            if now - self.private_stream_healthy_at.get().unwrap_or(f64::NAN) >= 300000.0 {
                self.session_refresh_attempts.set(0.0);
                self.private_stream_healthy_at.set(None);
            }
        }
        Ok(())
    }
    /// ts: GrvtPrivateWebSocket#failClosedForAuthentication. Emit failure leaves terminal flags set.
    pub fn fail_closed_for_authentication(
        &self,
        reason: AuthenticationFailureReason,
        mut emit: impl FnMut(&str) -> Result<(), ExchangeError>,
        mut dispose: impl FnMut() -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.authenticated.set(false);
        self.authentication_failed.set(true);
        emit(&format!(
            "GRVT private WebSocket authentication failed closed ({}). Check the API key and account authorization, then reconnect.",
            reason.as_str()
        ))?;
        dispose()
    }
    /// ts: GrvtPrivateWebSocket#subscribeToStream. Counter/insertion happen before send.
    pub fn subscribe_to_stream(
        &self,
        stream: &str,
        feed: StreamFeed,
        mut send: impl FnMut(&LegacySubscription, &str) -> Result<(), ExchangeError>,
    ) -> Result<f64, ExchangeError> {
        let id = self.request_id.get();
        self.request_id.set(id + 1.0);
        let request = LegacySubscription {
            request_id: id,
            method: crate::types::GrvtSubscriptionMethod::Subscribe,
            stream: stream.into(),
            feed,
            is_full: false,
        };
        self.subscriptions
            .borrow_mut()
            .insert(NumberKey::new(id), request.clone());
        send(&request, &request.wire()?)?;
        Ok(id)
    }
}
impl<T: Clone, F: Eq + Hash> PrivateCommands<T, F> {
    /// ts: GrvtPrivateWebSocket.dispose. Maps clear before timer cancellation, then base disposal.
    pub fn dispose(
        &self,
        timer_is_truthy: impl Fn(&T) -> bool,
        mut clear: impl FnMut(&T) -> Result<(), ExchangeError>,
        mut base: impl FnMut() -> Result<(), ExchangeError>,
    ) -> Result<(), ExchangeError> {
        self.subscriptions.borrow_mut().clear();
        self.processed_fill_ids.borrow_mut().clear();
        let timeout = self.orders_throttle_timeout.borrow().clone();
        if let Some(timeout) = timeout.filter(|timer| timer_is_truthy(timer)) {
            clear(&timeout)?;
            *self.orders_throttle_timeout.borrow_mut() = None;
        }
        self.private_stream_healthy_at.set(None);
        base()
    }
}
