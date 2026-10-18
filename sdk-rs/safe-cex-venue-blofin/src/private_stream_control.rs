//! Private socket control bodies; facade and the three data handlers remain separate.
use crate::utility::UtilityInput;
use base64::{Engine, engine::general_purpose::STANDARD};
use safe_cex_crypto::{Encoding, hmac_sha256};
use safe_cex_num::{
    JsJsonValue, js_json_stringify, js_number_string, js_string, js_truthy,
    numeric_ingress::js_from_str,
};
use safe_cex_store::{Store, StoreUpdate};
use safe_cex_types::{Order, js_coercion::JsCoercionError};
use serde_json::Value;
use std::{
    cell::{Cell, RefCell},
    future::{Future, poll_fn},
    pin::Pin,
    rc::Rc,
    task::Poll,
};

pub type PrivateStreamFuture<T> = Pin<Box<dyn Future<Output = T>>>;
pub type PrivateStreamStart<T, E> = Result<PrivateStreamFuture<Result<T, E>>, E>;
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PrivateAuthError<E> {
    Host(E),
    Coercion(JsCoercionError),
}
impl<E: std::fmt::Display> std::fmt::Display for PrivateAuthError<E> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Host(e) => e.fmt(f),
            Self::Coercion(e) => f.write_str(e.message),
        }
    }
}
impl<E: std::error::Error> std::error::Error for PrivateAuthError<E> {}
pub struct PrivateStreamState<Timer> {
    pub ping_at: Cell<f64>,
    pub orders_throttle_timeout: RefCell<Option<Timer>>,
}
impl<Timer> Default for PrivateStreamState<Timer> {
    fn default() -> Self {
        Self {
            ping_at: Cell::new(0.0),
            orders_throttle_timeout: RefCell::new(None),
        }
    }
}
pub struct PrivateRoute {
    pub tunnelled: bool,
    pub host: Option<String>,
}
pub struct RemoteAuthRequest {
    pub kind: &'static str,
    pub exchange_name: &'static str,
    pub output_format: &'static str,
    pub id: String,
    pub account_name: Option<Value>,
    pub to_sign: String,
    pub timestamp: f64,
    pub nonce: String,
}
pub struct RemoteAuthResult {
    pub error: Option<Value>,
    pub signature: Option<Value>,
}

/// Synchronous base, routing, signing and Store boundaries. Deferred timers and
/// task launchers must retain/start operations through completion, as JS promises
/// do; this slice does not compose the pending facade or inbound data handlers.
pub trait PrivateStreamHost: 'static {
    type Error: 'static;
    type Timer: Clone + 'static;
    fn is_disposed(&self) -> bool;
    fn is_testnet(&self) -> bool;
    fn resolve_private_route(&self) -> Result<PrivateRoute, Self::Error>;
    fn create_and_assign_socket(&self, url: Option<&str>) -> Result<(), Self::Error>;
    fn add_socket_listener(&self, event: &str) -> Result<(), Self::Error>;
    fn super_dispose(&self) -> Result<(), Self::Error>;
    fn timer_is_truthy(&self, timer: &Self::Timer) -> bool;
    fn clear_timeout(&self, timer: &Self::Timer) -> Result<(), Self::Error>;
    fn set_timeout(
        &self,
        callback: Rc<dyn Fn() -> PrivateStreamFuture<()>>,
        delay_ms: f64,
    ) -> Result<Self::Timer, Self::Error>;
    fn send_with_log(&self, frame: &str) -> Result<(), Self::Error>;
    fn performance_now(&self) -> f64;
    fn nonce(&self) -> String;
    fn timestamp_ms(&self) -> f64;
    fn remote_signer_present(&self) -> bool;
    fn remote_account_name(&self) -> Option<Value>;
    fn request_id(&self) -> String;
    fn sign_remote(
        &self,
        request: RemoteAuthRequest,
    ) -> PrivateStreamStart<RemoteAuthResult, Self::Error>;
    fn secret(&self) -> String;
    fn key(&self) -> String;
    fn passphrase(&self) -> Option<Value>;
    fn emit_signing_error(&self, message: &str) -> Result<(), Self::Error>;
    fn launch_auth(&self, task: PrivateStreamFuture<Result<(), PrivateAuthError<Self::Error>>>);
    fn track_last_received(&self) -> Result<(), Self::Error>;
    fn diagnostic_receive(&self, data: &str) -> Result<(), Self::Error>;
    fn log_last_log(&self, data: &str) -> Result<(), Self::Error>;
    fn handle_pong(&self) -> Result<(), Self::Error>;
    fn reconnecting_enabled(&self) -> bool;
    fn fetch_and_set_private_snapshot(&self) -> Result<(), Self::Error>;
    fn handle_order_topic(&self, data: &Value) -> Result<(), Self::Error>;
    fn handle_position_topic(&self, data: &Value) -> Result<(), Self::Error>;
    fn handle_account_topic(&self, data: &Value) -> Result<(), Self::Error>;
    fn fetch_orders(&self) -> PrivateStreamStart<Vec<Order>, Self::Error>;
    fn store(&self) -> &dyn Store;
}

pub fn endpoint<H: PrivateStreamHost>(host: &H) -> &'static str {
    crate::types::BASE_WS_URL["private"][if host.is_testnet() {
        "testnet"
    } else {
        "livenet"
    }]
    .as_str()
    .expect("declared private URL")
}
pub fn dispose<H: PrivateStreamHost>(
    state: &PrivateStreamState<H::Timer>,
    host: &H,
) -> Result<(), H::Error> {
    let timeout = state.orders_throttle_timeout.borrow().clone();
    if let Some(timer) = timeout.filter(|timer| host.timer_is_truthy(timer)) {
        host.clear_timeout(&timer)?;
        *state.orders_throttle_timeout.borrow_mut() = None;
    }
    host.super_dispose()
}
pub fn connect_and_subscribe<H: PrivateStreamHost>(host: &H) -> Result<(), H::Error> {
    if !host.is_disposed() {
        let route = host.resolve_private_route()?;
        let url = if route.tunnelled {
            route.host.as_deref()
        } else {
            Some(endpoint(host))
        };
        host.create_and_assign_socket(url)?;
        for event in ["open", "message", "close"] {
            host.add_socket_listener(event)?;
        }
    }
    Ok(())
}
pub fn on_open<H: PrivateStreamHost>(
    state: &PrivateStreamState<H::Timer>,
    host: &Rc<H>,
) -> Result<(), H::Error> {
    if !host.is_disposed() {
        let task = auth(host);
        host.launch_auth(task);
        ping(state, host.as_ref())?;
    }
    Ok(())
}
pub fn ping<H: PrivateStreamHost>(
    state: &PrivateStreamState<H::Timer>,
    host: &H,
) -> Result<(), H::Error> {
    if !host.is_disposed() {
        state.ping_at.set(host.performance_now());
        host.send_with_log("ping")?;
    }
    Ok(())
}
async fn after_await() {
    let mut yielded = false;
    poll_fn(|cx| {
        if yielded {
            Poll::Ready(())
        } else {
            yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    })
    .await;
}
fn stringify(value: JsJsonValue) -> String {
    js_json_stringify(&value)
        .expect("owned JSON-only auth data")
        .expect("object frame defined")
}
fn login<H: PrivateStreamHost>(
    host: &H,
    timestamp: f64,
    nonce: String,
    signature: JsJsonValue,
) -> Result<(), H::Error> {
    let credentials = JsJsonValue::Object(vec![
        ("apiKey".into(), JsJsonValue::String(host.key())),
        (
            "passphrase".into(),
            host.passphrase()
                .map_or(JsJsonValue::Undefined, JsJsonValue::from),
        ),
        ("timestamp".into(), JsJsonValue::Number(timestamp)),
        ("sign".into(), signature),
        ("nonce".into(), JsJsonValue::String(nonce)),
    ]);
    host.send_with_log(&stringify(JsJsonValue::Object(vec![
        ("op".into(), JsJsonValue::String("login".into())),
        ("args".into(), JsJsonValue::Array(vec![credentials])),
    ])))
}
/// Starts the synchronous prefix immediately, then exposes only the continuation
/// as a future. Calling an async Rust body directly would incorrectly delay login
/// until after onOpen's ping. Remote completion always crosses a JS await turn.
pub fn auth<H: PrivateStreamHost>(
    host: &Rc<H>,
) -> PrivateStreamFuture<Result<(), PrivateAuthError<H::Error>>> {
    let nonce = host.nonce();
    let timestamp = host.timestamp_ms();
    if host.remote_signer_present() {
        let request = RemoteAuthRequest {
            kind: "ws-auth",
            exchange_name: "blofin",
            output_format: "hex-base64",
            id: host.request_id(),
            account_name: host.remote_account_name(),
            to_sign: format!(
                "/users/self/verifyGET{}{}",
                js_number_string(timestamp),
                nonce
            ),
            timestamp,
            nonce: nonce.clone(),
        };
        let request = host.sign_remote(request);
        let host = host.clone();
        return Box::pin(async move {
            let result = request
                .map_err(PrivateAuthError::Host)?
                .await
                .map_err(PrivateAuthError::Host)?;
            after_await().await;
            if js_truthy(result.error.as_ref()) {
                let text = js_string(result.error.as_ref()).map_err(PrivateAuthError::Coercion)?;
                host.emit_signing_error(&format!("Remote WS signing unavailable: {text}"))
                    .map_err(PrivateAuthError::Host)?;
                return Ok(());
            }
            login(
                host.as_ref(),
                timestamp,
                nonce,
                result
                    .signature
                    .map_or(JsJsonValue::Undefined, JsJsonValue::from),
            )
            .map_err(PrivateAuthError::Host)
        });
    }
    let payload = format!(
        "/users/self/verifyGET{}{}",
        js_number_string(timestamp),
        nonce
    );
    let signature = STANDARD.encode(hmac_sha256(
        host.secret().as_bytes(),
        payload.as_bytes(),
        Encoding::Hex,
    ));
    let result = login(
        host.as_ref(),
        timestamp,
        nonce,
        JsJsonValue::String(signature),
    );
    Box::pin(async move { result.map_err(PrivateAuthError::Host) })
}
pub fn subscribe<H: PrivateStreamHost>(host: &H) -> Result<(), H::Error> {
    for channel in ["account", "positions", "orders", "orders-algo"] {
        host.send_with_log(&stringify(JsJsonValue::Object(vec![
            ("op".into(), JsJsonValue::String("subscribe".into())),
            (
                "args".into(),
                JsJsonValue::Array(vec![JsJsonValue::Object(vec![(
                    "channel".into(),
                    JsJsonValue::String(channel.into()),
                )])]),
            ),
        ])))?;
    }
    Ok(())
}
pub fn on_message<H: PrivateStreamHost>(host: &H, data: &str) -> Result<(), H::Error> {
    if !host.is_disposed() {
        host.track_last_received()?;
        host.diagnostic_receive(data)?;
        host.log_last_log(data)?;
        if data.contains("event\":\"subscribe\"") {
            return Ok(());
        }
        if data == "pong" {
            return host.handle_pong();
        }
        if data == "{\"event\":\"login\",\"code\":\"0\",\"msg\":\"\"}" {
            subscribe(host)?;
            if host.reconnecting_enabled() {
                host.fetch_and_set_private_snapshot()?;
            }
            return Ok(());
        }
        let handler = if data.contains("\"channel\":\"orders\"")
            || data.contains("\"channel\":\"orders-algo\"")
        {
            Some(0)
        } else if data.contains("\"channel\":\"positions\"") {
            Some(1)
        } else if data.contains("\"channel\":\"account\"") {
            Some(2)
        } else {
            None
        };
        if let Some(handler) = handler
            && let Ok(parsed) = js_from_str::<UtilityInput>(data)
            && js_truthy(Some(parsed.json()))
        {
            match handler {
                0 => host.handle_order_topic(parsed.json())?,
                1 => host.handle_position_topic(parsed.json())?,
                _ => host.handle_account_topic(parsed.json())?,
            }
        }
    }
    Ok(())
}
pub fn throttled_fetch_orders<H: PrivateStreamHost>(
    state: &Rc<PrivateStreamState<H::Timer>>,
    host: &Rc<H>,
) -> Result<(), H::Error> {
    let timeout = state.orders_throttle_timeout.borrow().clone();
    if let Some(timer) = timeout.filter(|timer| host.timer_is_truthy(timer)) {
        host.clear_timeout(&timer)?;
    }
    let callback_state = state.clone();
    let callback_host = host.clone();
    let callback = Rc::new(move || fetch_throttled_orders(&callback_state, &callback_host));
    let timer = host.set_timeout(callback, 2000.0)?;
    *state.orders_throttle_timeout.borrow_mut() = Some(timer);
    Ok(())
}
pub fn fetch_throttled_orders<H: PrivateStreamHost>(
    state: &PrivateStreamState<H::Timer>,
    host: &Rc<H>,
) -> PrivateStreamFuture<()> {
    *state.orders_throttle_timeout.borrow_mut() = None;
    if host.is_disposed() {
        return Box::pin(async {});
    }
    let request = host.fetch_orders();
    let host = host.clone();
    Box::pin(async move {
        let Ok(request) = request else {
            return;
        };
        let Ok(orders) = request.await else {
            return;
        };
        after_await().await;
        // Source fetchThrottledOrders catches the entire fetch/update block
        // (blofin.ws-private.ts:273-287), including a loaded getter failure.
        let Ok(mut loaded) = host.store().loaded() else {
            return;
        };
        loaded.orders = true;
        let _ = host.store().update(
            StoreUpdate {
                orders: Some(orders),
                loaded: Some(loaded),
                ..Default::default()
            },
            false,
        );
    })
}
