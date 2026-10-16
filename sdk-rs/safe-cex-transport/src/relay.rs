use crate::{
    TransportError,
    socket::{SocketFrame, TunnelDialOptions, TunnelHost, TunnelSocket, within_deadline},
};
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::VecDeque,
    rc::Rc,
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum RelayState {
    Connecting,
    Open,
    Closing,
    Closed,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RelayEvent {
    Open,
    Message(SocketFrame),
    Error(String),
    Close {
        code: u16,
        reason: String,
        was_clean: bool,
    },
}

pub type FreshHeadersProvider = Rc<dyn Fn() -> indexmap::IndexMap<String, String>>;

#[derive(Clone)]
pub enum UpgradeHeaders {
    Static(indexmap::IndexMap<String, String>),
    Fresh(FreshHeadersProvider),
}

#[derive(Clone, Default)]
pub struct WebSocketConnectionOptions {
    // Optional RWS tuning: absence uses the wrapper's library defaults.
    pub min_reconnection_delay_ms: Option<f64>,
    pub max_reconnection_delay_ms: Option<f64>,
    pub reconnection_delay_grow_factor: Option<f64>,
    pub min_uptime_ms: Option<f64>,
    pub connection_timeout_ms: Option<f64>,
    pub max_retries: Option<f64>,
    pub start_closed: Option<bool>,
    pub headers: Option<UpgradeHeaders>,
    pub headers_required: bool,
    pub protocols: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HeaderCapableWebSocketUnavailableError;

impl std::fmt::Display for HeaderCapableWebSocketUnavailableError {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str("Required WebSocket upgrade headers need a header-capable ws transport")
    }
}
impl std::error::Error for HeaderCapableWebSocketUnavailableError {}

type EventHandler = Rc<dyn Fn(&RelayEvent)>;

pub struct RelayWebSocket {
    host: Rc<dyn TunnelHost>,
    connection_options: WebSocketConnectionOptions,
    tunnel_url: String,
    upstream: String,
    state: Cell<RelayState>,
    socket: RefCell<Option<Rc<dyn TunnelSocket>>>,
    queue: RefCell<VecDeque<SocketFrame>>,
    events: RefCell<VecDeque<RelayEvent>>,
    auth_sent: Cell<bool>,
    initializing: Cell<bool>,
    polling: Cell<bool>,
    close_request: RefCell<Option<(Option<u16>, String)>>,
    deadline: u64,
    close_dispatched: Cell<bool>,
    on_open: RefCell<Option<EventHandler>>,
    on_close: RefCell<Option<EventHandler>>,
    on_message: RefCell<Option<EventHandler>>,
    on_error: RefCell<Option<EventHandler>>,
}

struct RelayInitializing<'a> {
    relay: &'a RelayWebSocket,
    complete: bool,
}
impl Drop for RelayInitializing<'_> {
    fn drop(&mut self) {
        self.relay.initializing.set(false);
        if !self.complete {
            self.relay.fail("relay initialization cancelled".into());
        }
    }
}
struct RelayPolling<'a>(&'a Cell<bool>);
impl Drop for RelayPolling<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

pub fn parse_relay_url(url: &str) -> Result<(String, String), TransportError> {
    crate::tunnel_endpoints::parse_relay_url(url)
}

pub(crate) fn decode_query(value: &str) -> String {
    let mut out = Vec::new();
    let mut bytes = value.bytes().peekable();
    while let Some(byte) = bytes.next() {
        if byte == b'+' {
            out.push(b' ');
        } else if byte == b'%' {
            let mut lookahead = bytes.clone();
            if let (Some(a), Some(b)) = (lookahead.next(), lookahead.next())
                && let (Some(a), Some(b)) = (char::from(a).to_digit(16), char::from(b).to_digit(16))
            {
                out.push((a * 16 + b) as u8);
                bytes = lookahead;
                continue;
            }
            out.push(byte);
        } else {
            out.push(byte);
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}

impl RelayWebSocket {
    pub fn new(host: Rc<dyn TunnelHost>, url: &str) -> Result<Self, TransportError> {
        Self::with_connection_options(host, url, WebSocketConnectionOptions::default())
    }

    pub fn with_connection_options(
        host: Rc<dyn TunnelHost>,
        url: &str,
        connection_options: WebSocketConnectionOptions,
    ) -> Result<Self, TransportError> {
        Self::validate_connection_headers(host.as_ref(), &connection_options)?;
        let (tunnel_url, upstream) = parse_relay_url(url)?;
        TunnelDialOptions::new(host.capabilities())?;
        let deadline = host.now_ms().saturating_add(15_000);
        Ok(Self {
            host,
            connection_options,
            tunnel_url,
            upstream,
            deadline,
            state: Cell::new(RelayState::Connecting),
            socket: RefCell::new(None),
            queue: RefCell::new(VecDeque::new()),
            events: RefCell::new(VecDeque::new()),
            auth_sent: Cell::new(false),
            initializing: Cell::new(false),
            polling: Cell::new(false),
            close_request: RefCell::new(None),
            close_dispatched: Cell::new(false),
            on_open: RefCell::new(None),
            on_close: RefCell::new(None),
            on_message: RefCell::new(None),
            on_error: RefCell::new(None),
        })
    }

    fn validate_connection_headers(
        host: &dyn TunnelHost,
        connection_options: &WebSocketConnectionOptions,
    ) -> Result<(), TransportError> {
        let capabilities = host.capabilities();
        if connection_options.headers_required
            && (cfg!(target_arch = "wasm32")
                || !capabilities.upgrade_headers
                || matches!(
                    capabilities.runtime,
                    crate::socket::SocketRuntime::Browser
                        | crate::socket::SocketRuntime::BrowserWorker
                        | crate::socket::SocketRuntime::BunNative
                ))
        {
            return Err(TransportError::UnavailableTransport(
                HeaderCapableWebSocketUnavailableError.to_string(),
            ));
        }
        Ok(())
    }

    pub fn set_on_open(&self, callback: Option<EventHandler>) {
        *self.on_open.borrow_mut() = callback;
    }
    pub fn set_on_close(&self, callback: Option<EventHandler>) {
        *self.on_close.borrow_mut() = callback;
    }
    pub fn set_on_message(&self, callback: Option<EventHandler>) {
        *self.on_message.borrow_mut() = callback;
    }
    pub fn set_on_error(&self, callback: Option<EventHandler>) {
        *self.on_error.borrow_mut() = callback;
    }
    /// WebSocket readyState, using the existing relay state machine.
    pub fn ready_state(&self) -> u8 {
        match self.state.get() {
            RelayState::Connecting => 0,
            RelayState::Open => 1,
            RelayState::Closing => 2,
            RelayState::Closed => 3,
        }
    }

    pub fn buffered_amount(&self) -> u64 {
        self.socket
            .borrow()
            .as_ref()
            .map_or(0, |socket| socket.buffered_amount())
    }

    fn dispatch_event(&self, event: RelayEvent) {
        let callback = match &event {
            RelayEvent::Open => self.on_open.borrow().clone(),
            RelayEvent::Close { .. } => self.on_close.borrow().clone(),
            RelayEvent::Message(_) => self.on_message.borrow().clone(),
            RelayEvent::Error(_) => self.on_error.borrow().clone(),
        };
        if let Some(callback) = callback {
            callback(&event);
        }
        self.events.borrow_mut().push_back(event);
    }
    fn dispatch_message(&self, frame: SocketFrame) {
        self.dispatch_event(RelayEvent::Message(frame));
    }
    fn dispatch_error(&self, message: String) {
        self.dispatch_event(RelayEvent::Error(message));
    }

    pub fn send(&self, data: SocketFrame) -> Result<(), TransportError> {
        if matches!(data, SocketFrame::Close { .. }) {
            return Err(TransportError::Host(
                "close events cannot be sent as data".into(),
            ));
        }
        match self.state.get() {
            RelayState::Open => self.inner_send(data),
            RelayState::Connecting => {
                let mut queue = self.queue.borrow_mut();
                if queue.len() >= 100 {
                    queue.pop_front();
                }
                queue.push_back(data);
                Ok(())
            }
            _ => Err(TransportError::Host("RelayWebSocket is not open".into())),
        }
    }

    pub async fn on_inner_open(&self) -> Result<(), TransportError> {
        if !matches!(
            self.state.get(),
            RelayState::Connecting | RelayState::Closing
        ) {
            return Ok(());
        }
        if self.socket.borrow().is_some() {
            return Ok(());
        }
        Self::validate_connection_headers(self.host.as_ref(), &self.connection_options)?;
        if self.initializing.replace(true) {
            return Err(TransportError::Host(
                "relay initialization already in progress".into(),
            ));
        }
        let mut initializing = RelayInitializing {
            relay: self,
            complete: false,
        };
        let result = async {
            let mut options = TunnelDialOptions::new(self.host.capabilities())?;
            if !cfg!(target_arch = "wasm32")
                && !matches!(
                    self.host.capabilities().runtime,
                    crate::socket::SocketRuntime::Browser
                        | crate::socket::SocketRuntime::BrowserWorker
                )
            {
                let headers = match &self.connection_options.headers {
                    Some(UpgradeHeaders::Static(headers)) => headers.clone(),
                    Some(UpgradeHeaders::Fresh(provider)) => provider(),
                    None => indexmap::IndexMap::new(),
                };
                options.overlay(headers, self.connection_options.protocols.clone());
            }
            let socket = within_deadline(
                self.host.as_ref(),
                if self.state.get() == RelayState::Closing {
                    u64::MAX
                } else {
                    self.deadline
                },
                self.host.connect(self.tunnel_url.clone(), options),
            )
            .await?;
            if self.state.get() != RelayState::Connecting {
                if self.state.get() == RelayState::Closing {
                    let request = self.close_request.borrow().clone();
                    let result = socket.close(
                        request.as_ref().and_then(|(code, _)| *code),
                        request.as_ref().map_or("", |(_, reason)| reason.as_str()),
                    );
                    *self.socket.borrow_mut() = Some(socket);
                    if result.is_err() {
                        self.finish(1006, "close failed".into(), false);
                    }
                } else {
                    let _ = socket.close(None, "");
                }
                return Ok(());
            }
            *self.socket.borrow_mut() = Some(socket);
            let token = within_deadline(
                self.host.as_ref(),
                self.deadline,
                crate::socket::proxy_token(self.host.as_ref()),
            )
            .await
            .map_err(|_| TransportError::Host("proxy token unavailable".into()))?;
            if self.state.get() != RelayState::Connecting {
                return Ok(());
            }
            self.auth_sent.set(true);
            self.inner_send(SocketFrame::Text(
                json!({"type":"rpc.auth", "token":token, "source":"worker"}).to_string(),
            ))
            .map_err(|_| TransportError::Host("proxy token unavailable".into()))
        }
        .await;
        initializing.complete = true;
        if let Err(error) = result {
            self.fail(error.to_string());
        }
        Ok(())
    }

    pub fn on_inner_message(&self, frame: SocketFrame) -> Result<(), TransportError> {
        if let SocketFrame::Close {
            code,
            reason,
            was_clean,
        } = frame
        {
            self.on_inner_close(code, reason, was_clean);
            return Ok(());
        }
        if self.state.get() == RelayState::Open {
            self.dispatch_message(frame);
            return Ok(());
        }
        if self.state.get() != RelayState::Connecting {
            return Ok(());
        }
        let SocketFrame::Text(text) = frame else {
            return Ok(());
        };
        let Ok(value) = serde_json::from_str::<Value>(&text) else {
            return Ok(());
        };
        match value["type"].as_str() {
            Some("rpc.auth") if self.auth_sent.get() => {
                if value["status"] == "success" {
                    if let Err(error) = self.inner_send(SocketFrame::Text(
                        json!({"type":"rpc.relay", "upstream":self.upstream}).to_string(),
                    )) {
                        self.fail(error.to_string());
                    }
                } else {
                    self.fail(format!(
                        "tunnel auth failed: {}",
                        relay_error_text(value.get("error"))
                    ));
                }
            }
            Some("rpc.relay") => {
                if value["status"] == "open" {
                    self.become_open()?;
                } else {
                    self.fail(format!(
                        "relay refused: {}",
                        relay_error_text(value.get("error"))
                    ));
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub async fn next_event(&self) -> Result<RelayEvent, TransportError> {
        if self.polling.replace(true) {
            return Err(TransportError::Host(
                "relay event consumer already active".into(),
            ));
        }
        let _polling = RelayPolling(&self.polling);
        loop {
            if let Some(event) = self.events.borrow_mut().pop_front() {
                return Ok(event);
            }
            if self.state.get() == RelayState::Closed {
                return Err(TransportError::Host("RelayWebSocket is closed".into()));
            }
            if self.socket.borrow().is_none() {
                self.on_inner_open().await?;
                continue;
            }
            let socket = self.socket.borrow().clone().expect("socket initialized");
            let received = if self.state.get() == RelayState::Connecting {
                let mut receive = socket.receive();
                let mut timer = self.host.sleep_until(self.deadline);
                std::future::poll_fn(|cx| {
                    // Closing/open transitions cancel this timer even if receive was already parked.
                    if self.state.get() == RelayState::Connecting
                        && timer.as_mut().poll(cx).is_ready()
                    {
                        return std::task::Poll::Ready(Err(TransportError::Timeout));
                    }
                    receive.as_mut().poll(cx)
                })
                .await
            } else {
                socket.receive().await
            };
            match received {
                Ok(frame) => self.on_inner_message(frame)?,
                Err(TransportError::Timeout) if self.state.get() == RelayState::Connecting => {
                    self.fail("relay handshake timed out".into());
                }
                Err(_) => self.on_inner_error(),
            }
        }
    }

    pub fn close(&self, code: impl Into<Option<u16>>, reason: &str) {
        let code = code.into();
        if matches!(self.state.get(), RelayState::Closed | RelayState::Closing) {
            return;
        }
        self.state.set(RelayState::Closing);
        *self.close_request.borrow_mut() = Some((code, reason.to_owned()));
        let socket = self.socket.borrow().clone();
        if let Some(socket) = socket
            && socket.close(code, reason).is_err()
        {
            self.finish(1006, "close failed".into(), false);
        }
    }

    fn inner_send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        self.socket
            .borrow()
            .as_ref()
            .ok_or_else(|| TransportError::Host("relay transport not connected".into()))?
            .send(frame)
    }

    fn become_open(&self) -> Result<(), TransportError> {
        self.state.set(RelayState::Open);
        self.dispatch_event(RelayEvent::Open);
        // ts-bug: becomeOpen drains the entire queue before sending; a throw loses the tail.
        let queued = std::mem::take(&mut *self.queue.borrow_mut());
        for frame in queued {
            self.inner_send(frame)?;
        }
        Ok(())
    }

    fn fail(&self, message: String) {
        if self.state.get() == RelayState::Closed {
            return;
        }
        self.dispatch_error(message.clone());
        self.state.set(RelayState::Closing);
        if let Some(socket) = self.socket.borrow().clone() {
            let _ = socket.close(None, "");
        }
        self.finish(4000, message, false);
    }

    pub fn on_inner_error(&self) {
        if self.state.get() != RelayState::Closed {
            self.dispatch_error("relay transport error".into());
        }
    }

    pub fn on_inner_close(&self, code: u16, reason: String, was_clean: bool) {
        self.socket.borrow_mut().take();
        self.finish(code, reason, was_clean);
    }

    fn finish(&self, code: u16, reason: String, was_clean: bool) {
        if self.close_dispatched.replace(true) {
            return;
        }
        self.state.set(RelayState::Closed);
        self.dispatch_event(RelayEvent::Close {
            code,
            reason,
            was_clean,
        });
    }
}
impl Drop for RelayWebSocket {
    fn drop(&mut self) {
        self.close(None, "");
    }
}

// Template interpolation follows String(value), not JSON.stringify(value).
fn relay_error_text(value: Option<&serde_json::Value>) -> String {
    fn string(value: &serde_json::Value) -> String {
        match value {
            serde_json::Value::String(value) => value.clone(),
            serde_json::Value::Object(_) => "[object Object]".into(),
            serde_json::Value::Array(values) => values
                .iter()
                .map(|value| {
                    if value.is_null() {
                        String::new()
                    } else {
                        string(value)
                    }
                })
                .collect::<Vec<_>>()
                .join(","),
            serde_json::Value::Number(value) => safe_cex_num::js_number_string(
                value.to_string().parse::<f64>().expect("JSON number"),
            ),
            _ => value.to_string(),
        }
    }
    value
        .filter(|value| !value.is_null())
        .map_or_else(|| "unknown".into(), string)
}
