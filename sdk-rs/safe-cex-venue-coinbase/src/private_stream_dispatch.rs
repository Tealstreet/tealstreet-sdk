//! Private websocket lifecycle, auth subscriptions and user-event dispatch.
//! Host capabilities bind existing BaseWebSocket, routing, signer, health and order methods.
use crate::{
    portfolios_balances::{PortfolioFailure, PortfolioThrown},
    public_stream_dispatch::{PublicSocket, PublicSocketListener},
    starbase_auth::{
        CoinbaseStarbasePromise as Promise, CoinbaseStarbasePromiseSpawner, queue_job,
    },
    starbase_public_frames::CoinbaseStarbaseFrameValue as Frame,
};
use safe_cex_num::{js_string, js_truthy};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    rc::Rc,
};

#[derive(Debug, Clone, PartialEq)]
pub struct PrivateWsAuthRequest {
    pub id: String,
    pub request_type: &'static str,
    pub exchange_name: String,
    /// Own accountName property: None retains undefined before the host wire boundary.
    pub account_name: Option<Value>,
    pub to_sign: String,
    pub output_format: &'static str,
    pub channel: String,
}
pub struct PrivateRemoteSigner {
    pub account_name: RefCell<Option<Value>>,
    pub sign: Rc<
        dyn Fn(
            PrivateWsAuthRequest,
        )
            -> Result<Promise<Result<Option<Value>, PortfolioFailure>>, PortfolioFailure>,
    >,
}
#[derive(Clone)]
pub struct PrivateStreamOptions {
    pub key: Option<Value>,
    pub secret: Option<Value>,
    pub remote_signer: Option<Rc<PrivateRemoteSigner>>,
}
pub type PrivateOptions = Rc<RefCell<PrivateStreamOptions>>;
#[derive(Clone)]
pub struct PrivateStreamRoute {
    pub tunnelled: bool,
    pub host: Option<Value>,
}
#[derive(Debug, Clone, PartialEq)]
pub struct PrivateSubscription {
    pub request_type: &'static str,
    pub channel: &'static str,
    pub jwt: Value,
}
pub trait PrivateStreamHost {
    fn is_disposed(&self) -> bool;
    fn options(&self) -> PrivateOptions;
    fn exchange_name(&self) -> String;
    fn testnet(&self) -> bool;
    fn resolve_private_route(
        &self,
        exchange_name: String,
        options: PrivateOptions,
    ) -> Result<PrivateStreamRoute, PortfolioFailure>;
    fn create_websocket(&self, url: Option<Value>) -> Result<PublicSocket, PortfolioFailure>;
    fn add_listener(
        &self,
        socket: PublicSocket,
        event: PublicSocketListener,
    ) -> Result<(), PortfolioFailure>;
    fn reset_order_snapshot_assembly(&self) -> Result<(), PortfolioFailure>;
    fn close_socket(&self, socket: PublicSocket) -> Result<(), PortfolioFailure>;
    fn send_with_log(&self, payload: PrivateSubscription) -> Result<(), PortfolioFailure>;
    fn sign_local(
        &self,
        key: Option<Value>,
        secret: Option<Value>,
    ) -> Promise<Result<Value, PortfolioFailure>>;
    fn track_last_received(&self) -> Result<(), PortfolioFailure>;
    fn emit_ws_diagnostic_receive(&self, raw: &str) -> Result<(), PortfolioFailure>;
    /// Source JSON.parse capability (not the public jsonParse null-on-error helper).
    fn json_parse(&self, raw: &str) -> Result<Frame, PortfolioFailure>;
    fn mark_private_user_stream_auth_failed(
        &self,
        message: Option<Frame>,
    ) -> Result<(), PortfolioFailure>;
    fn mark_private_user_stream_message_received(&self) -> Result<(), PortfolioFailure>;
    fn emit_and_log_error(&self, message: Option<Frame>) -> Result<(), PortfolioFailure>;
    fn handle_user_event_orders(&self, event: Frame) -> Result<(), PortfolioFailure>;
    fn handle_pong_event(&self) -> Result<(), PortfolioFailure>;
}
pub struct PrivateStreamReceiver<H> {
    pub host: H,
    pub spawn: CoinbaseStarbasePromiseSpawner,
    pub ws: RefCell<Option<PublicSocket>>,
    pub subscription_attempt: Cell<f64>,
}
fn failure(name: &str, message: &str) -> PortfolioFailure {
    Rc::new(PortfolioThrown::from_value(Some(
        json!({"name":name,"message":message}),
    )))
}
fn string_frame(value: &str) -> Frame {
    Frame::String(value.encode_utf16().collect())
}
fn truthy(v: Option<&Frame>) -> bool {
    match v {
        None | Some(Frame::Null) => false,
        Some(Frame::Bool(v)) => *v,
        Some(Frame::Number(v)) => *v != 0.0 && !v.is_nan(),
        Some(Frame::String(v)) => !v.is_empty(),
        _ => true,
    }
}
fn equals(v: Option<Frame>, text: &str) -> bool {
    matches!(v,Some(Frame::String(v)) if v.iter().copied().eq(text.encode_utf16()))
}
fn prop(value: &Frame, key: &str) -> Result<Option<Frame>, PortfolioFailure> {
    if matches!(value, Frame::Null | Frame::Undefined) {
        Err(failure(
            "TypeError",
            &format!("Cannot read properties of null (reading '{key}')"),
        ))
    } else {
        Ok(value.get(key))
    }
}
fn frame_string(value: Option<&Frame>) -> Result<Vec<u16>, PortfolioFailure> {
    Ok(match value {
        None | Some(Frame::Undefined) => "undefined".encode_utf16().collect(),
        Some(Frame::Null) => "null".encode_utf16().collect(),
        Some(Frame::Bool(v)) => v.to_string().encode_utf16().collect(),
        Some(Frame::Number(v)) => safe_cex_num::js_number_string(*v).encode_utf16().collect(),
        Some(Frame::String(v)) => v.clone(),
        Some(Frame::Array(v)) => {
            let mut result = Vec::new();
            for (i, v) in v.borrow().iter().enumerate() {
                if i > 0 {
                    result.push(44)
                }
                if !matches!(v, Frame::Null | Frame::Undefined) {
                    result.extend(frame_string(Some(v))?);
                }
            }
            result
        }
        Some(Frame::Object(v)) => {
            if v.borrow()
                .contains_key(&"toString".encode_utf16().collect::<Vec<_>>())
            {
                return Err(failure(
                    "TypeError",
                    "Cannot convert object to primitive value",
                ));
            }
            "[object Object]".encode_utf16().collect()
        }
    })
}
fn auth_failure(message: &[u16]) -> bool {
    // Non-u /i folds ASCII letters here; no UTF16/surrogate string conversion is required.
    let lower: Vec<u16> = message
        .iter()
        .map(|v| if (65..=90).contains(v) { v + 32 } else { *v })
        .collect();
    if [
        "auth",
        "authentication",
        "unauthorized",
        "permission",
        "credential",
    ]
    .iter()
    .any(|s| {
        let needle: Vec<u16> = s.encode_utf16().collect();
        lower.windows(needle.len()).any(|w| w == needle)
    }) {
        return true;
    }
    for index in 0..lower.len() {
        if lower.get(index..index + 3) != Some(&[97, 112, 105]) {
            continue;
        }
        let after = index + 3;
        if lower.get(after..after + 3) == Some(&[107, 101, 121]) {
            return true;
        }
        if let Some(c) = lower.get(after) {
            if (*c == 45
                || *c == 95
                || matches!(*c,9..=13|32|160|5760|8192..=8202|8232|8233|8239|8287|12288|65279))
                && lower.get(after + 1..after + 4) == Some(&[107, 101, 121])
            {
                return true;
            }
        }
    }
    false
}
fn response_prop(value: Option<&Value>, key: &str) -> Result<Option<Value>, PortfolioFailure> {
    match value {
        None | Some(Value::Null) => Err(failure(
            "TypeError",
            &format!(
                "Cannot read properties of {} (reading '{key}')",
                if value.is_none() { "undefined" } else { "null" }
            ),
        )),
        Some(v) => Ok(v.get(key).cloned()),
    }
}
fn error_message(error: &PortfolioFailure) -> Option<Frame> {
    error
        .value
        .as_ref()
        .and_then(|v| v.get("message"))
        .filter(|v| js_truthy(Some(v)))
        .map(Frame::from_json)
}

// ts: CoinbasePrivateWebsocket.connectAndSubscribe
pub fn connect_and_subscribe<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
) -> Result<(), PortfolioFailure> {
    if !r.host.is_disposed() {
        let opts = r.host.options().borrow().clone();
        if !js_truthy(opts.key.as_ref())
            || (!js_truthy(opts.secret.as_ref()) && opts.remote_signer.is_none())
        {
            r.host.emit_and_log_error(Some(string_frame(
                "Coinbase private WebSocket requires an API key and signing secret",
            )))?;
            return Ok(());
        }
        let route = r
            .host
            .resolve_private_route(r.host.exchange_name(), r.host.options())?;
        let url = if route.tunnelled {
            route.host
        } else {
            let _testnet = r.host.testnet();
            Some(Value::String(
                "wss://advanced-trade-ws-user.coinbase.com".into(),
            ))
        };
        let socket = r.host.create_websocket(url)?;
        *r.ws.borrow_mut() = Some(socket);
        for event in [
            PublicSocketListener::Open,
            PublicSocketListener::Message,
            PublicSocketListener::Close,
        ] {
            let socket = r.ws.borrow().clone().ok_or_else(|| {
                failure(
                    "TypeError",
                    "Cannot read properties of undefined (reading 'addEventListener')",
                )
            })?;
            r.host.add_listener(socket, event)?;
        }
    }
    Ok(())
}
// ts: CoinbasePrivateWebsocket.onOpen
pub fn on_open<H: PrivateStreamHost + 'static>(
    r: Rc<PrivateStreamReceiver<H>>,
) -> Promise<Result<(), PortfolioFailure>> {
    let reset_receiver = r.clone();
    on_open_with_reset(r, move || {
        reset_receiver.host.reset_order_snapshot_assembly()
    })
}
pub(crate) fn on_open_with_reset<H: PrivateStreamHost + 'static>(
    r: Rc<PrivateStreamReceiver<H>>,
    reset: impl FnOnce() -> Result<(), PortfolioFailure> + 'static,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            if r.host.is_disposed() {
                return Ok(());
            }
            let attempt = r.subscription_attempt.get() + 1.0;
            r.subscription_attempt.set(attempt);
            reset()?;
            match subscribe(r.clone(), attempt).await {
                Ok(()) => {
                    if r.host.is_disposed() || attempt != r.subscription_attempt.get() {
                        return Ok(());
                    }
                    Ok(())
                }
                Err(err) => {
                    r.host.emit_and_log_error(error_message(&err).or_else(|| {
                        Some(string_frame(
                            "Coinbase private WebSocket subscription signing failed",
                        ))
                    }))?;
                    let socket = r.ws.borrow().clone();
                    if let Some(socket) = socket {
                        r.host.close_socket(socket)?;
                    }
                    Ok(())
                }
            }
        }),
        &spawn,
    )
}
// ts: CoinbasePrivateWebsocket.onMessage
pub fn on_message<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
    raw: &str,
) -> Result<(), PortfolioFailure> {
    on_message_with_orders(r, raw, |event| r.host.handle_user_event_orders(event))
}
pub(crate) fn on_message_with_orders<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
    raw: &str,
    mut orders: impl FnMut(Frame) -> Result<(), PortfolioFailure>,
) -> Result<(), PortfolioFailure> {
    r.host.track_last_received()?;
    if !r.host.is_disposed() {
        r.host.emit_ws_diagnostic_receive(raw)?;
        let json = r.host.json_parse(raw)?;
        if equals(prop(&json, "type")?, "error") || truthy(prop(&json, "error")?.as_ref()) {
            let message = prop(&json, "message")?;
            let message = if truthy(message.as_ref()) {
                message
            } else {
                let error = prop(&json, "error")?;
                let nested = error
                    .as_ref()
                    .filter(|v| !matches!(v, Frame::Null | Frame::Undefined))
                    .and_then(|v| v.get("message"));
                if truthy(nested.as_ref()) {
                    nested
                } else {
                    Some(string_frame("Coinbase private WebSocket error"))
                }
            };
            handle_private_stream_error(r, message)?;
        } else if equals(prop(&json, "channel")?, "user") {
            handle_user_event_with_orders(r, Some(&json), &mut orders)?;
        } else if equals(prop(&json, "channel")?, "heartbeats") {
            r.host.handle_pong_event()?;
        }
    }
    Ok(())
}
// ts: CoinbasePrivateWebsocket.ping
pub fn ping<H>(_: &PrivateStreamReceiver<H>) {}
// ts: CoinbasePrivateWebsocket.handlePrivateStreamError
pub fn handle_private_stream_error<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
    message: Option<Frame>,
) -> Result<(), PortfolioFailure> {
    if auth_failure(&frame_string(message.as_ref())?) {
        r.host.mark_private_user_stream_auth_failed(message)
    } else {
        r.host.emit_and_log_error(message)
    }
}
// ts: CoinbasePrivateWebsocket.handleUserEvent
pub fn handle_user_event<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
    data: Option<&Frame>,
) -> Result<(), PortfolioFailure> {
    handle_user_event_with_orders(r, data, &mut |event| r.host.handle_user_event_orders(event))
}
fn handle_user_event_with_orders<H: PrivateStreamHost>(
    r: &PrivateStreamReceiver<H>,
    data: Option<&Frame>,
    orders: &mut impl FnMut(Frame) -> Result<(), PortfolioFailure>,
) -> Result<(), PortfolioFailure> {
    r.host.mark_private_user_stream_message_received()?;
    let events = data.and_then(|d| d.get("events"));
    if truthy(events.as_ref()) {
        let Some(Frame::Array(events)) = events else {
            return Err(failure("TypeError", "events.forEach is not a function"));
        };
        let length = events.borrow().len();
        for i in 0..length {
            let event = events.borrow().get(i).cloned();
            if let Some(event) = event {
                if truthy(prop(&event, "orders")?.as_ref()) {
                    orders(event)?;
                }
            }
        }
    }
    Ok(())
}
// ts: CoinbasePrivateWebsocket.handlePositionTopic
pub fn handle_position_topic<H>(_: &PrivateStreamReceiver<H>, _: &[Frame]) {}
// ts: CoinbasePrivateWebsocket.subscribe
pub fn subscribe<H: PrivateStreamHost + 'static>(
    r: Rc<PrivateStreamReceiver<H>>,
    attempt: f64,
) -> Promise<Result<(), PortfolioFailure>> {
    let spawn = r.spawn.clone();
    Promise::from_future(
        Box::pin(async move {
            for channel in ["user", "heartbeats"] {
                let jwt = build_subscription_jwt(r.clone(), channel.into()).await?;
                if r.host.is_disposed() || attempt != r.subscription_attempt.get() {
                    return Ok(());
                }
                r.host.send_with_log(PrivateSubscription {
                    request_type: "subscribe",
                    channel,
                    jwt,
                })?;
            }
            Ok(())
        }),
        &spawn,
    )
}
// Promise-return adoption performs the source thenable-resolution job followed by a reaction,
// rather than treating `return sign(...)` as `return await sign(...)`.
fn adopt<T: Clone + 'static>(
    child: Promise<T>,
    spawn: &CoinbaseStarbasePromiseSpawner,
) -> Promise<T> {
    let (promise, resolve) = Promise::pending();
    let jobs = spawn.clone();
    queue_job(spawn, move || {
        child.observe(&jobs, move |value| resolve.resolve(value))
    });
    promise
}
// ts: CoinbasePrivateWebsocket.buildSubscriptionJwt
pub fn build_subscription_jwt<H: PrivateStreamHost + 'static>(
    r: Rc<PrivateStreamReceiver<H>>,
    channel: String,
) -> Promise<Result<Value, PortfolioFailure>> {
    let opts = r.host.options().borrow().clone();
    let spawn = r.spawn.clone();
    let Some(remote) = opts.remote_signer else {
        return adopt(r.host.sign_local(opts.key, opts.secret), &spawn);
    };
    Promise::from_future(
        Box::pin(async move {
            let request = PrivateWsAuthRequest {
                id: format!("coinbase-ws-auth.{channel}"),
                request_type: "ws-auth",
                exchange_name: r.host.exchange_name(),
                account_name: remote.account_name.borrow().clone(),
                to_sign: format!("coinbase-websocket-jwt.{channel}"),
                output_format: "coinbase-jwt",
                channel,
            };
            let result = (remote.sign)(request)?.await?;
            let error = response_prop(result.as_ref(), "error")?;
            if js_truthy(error.as_ref())
                || !js_truthy(response_prop(result.as_ref(), "wsAuthSignature")?.as_ref())
            {
                let message = if js_truthy(error.as_ref()) {
                    js_string(error.as_ref()).map_err(|e| failure(&e.name, &e.message))?
                } else {
                    "Remote signer returned no Coinbase WebSocket JWT".into()
                };
                return Err(failure("Error", &message));
            }
            Ok(response_prop(result.as_ref(), "wsAuthSignature")?.expect("truthy signature"))
        }),
        &spawn,
    )
}
