use crate::how_config::{CreateAxiosHttpOverWebsocketAdapterOptions, JsonParser, LatencyEntry};
use crate::how_context::{HOWRequestError, HOWResponse, TradeTunnelTimeoutError};
use crate::native_data::{self, property, text};
use crate::{
    RestAdapter, RestRequest, RestResponse, TransportError,
    frames::{HttpRequestArgs, RpcHttpResponse},
    routing::{HowEgress, is_tokyo_egress, tunnel_endpoint},
    socket::{
        SocketFrame, TunnelDialOptions, TunnelHost, TunnelSocket, race_deadline, valid_token,
        within_deadline,
    },
};
use indexmap::{IndexMap, IndexSet};
use safe_cex_types::retained_json::RetainedJsonInput as Input;
use serde_json::{Value, json};
use std::{
    cell::{Cell, RefCell},
    collections::BTreeMap,
    rc::{Rc, Weak},
};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HOWWebSocketAuthState {
    Unknown,
    Good,
    Bad,
}

struct PendingResponse {
    raw: bool,
    parser: Option<JsonParser>,
    request_summary: Value,
    route: Value,
    result: Option<Result<RestResponse, TransportError>>,
}

struct State {
    socket: Option<Rc<dyn TunnelSocket>>,
    time_synced: bool,
    auth: HOWWebSocketAuthState,
    token_sent: bool,
    pending: BTreeMap<String, PendingResponse>,
}

pub struct HOWWebSocket {
    host: Rc<dyn TunnelHost>,
    url: String,
    state: RefCell<State>,
    preparing: Cell<bool>,
    preparation: RefCell<Option<Rc<Preparation>>>,
    receiving: Cell<bool>,
    nonce: Cell<u64>,
    disposed: Cell<bool>,
    simulated_closed: Cell<bool>,
    next_ping: Cell<u64>,
    last_ping: Cell<Option<u64>>,
    pong_deadline: Cell<Option<u64>>,
    tunnel_latency: Cell<Option<u64>>,
    monitor: RefCell<Option<Box<dyn crate::socket::TunnelTask>>>,
    generation: Cell<u64>,
    known_latency_domains: RefCell<IndexSet<String>>,
    used_domains: RefCell<IndexSet<String>>,
    latency_by_domain: RefCell<IndexMap<String, LatencyEntry>>,
    next_latency_ticks: RefCell<Vec<u64>>,
}

struct Preparation {
    outcome: RefCell<Option<Result<(), TransportError>>>,
    share_failure: Cell<bool>,
}
struct Preparing<'a> {
    client: &'a HOWWebSocket,
    attempt: Rc<Preparation>,
}
impl Drop for Preparing<'_> {
    fn drop(&mut self) {
        self.attempt.outcome.borrow_mut().get_or_insert(Ok(()));
        self.client.preparation.borrow_mut().take();
        self.client.preparing.set(false);
    }
}
struct MonitorLease {
    client: Weak<HOWWebSocket>,
    preparing: bool,
}
impl Drop for MonitorLease {
    fn drop(&mut self) {
        if let Some(client) = self.client.upgrade() {
            if self.preparing {
                client.preparing.set(false);
            } else {
                client.receiving.set(false);
            }
        }
    }
}
struct Receiving<'a>(&'a Cell<bool>);
impl Drop for Receiving<'_> {
    fn drop(&mut self) {
        self.0.set(false);
    }
}

struct Pending<'a> {
    client: &'a HOWWebSocket,
    nonce: String,
}
impl Drop for Pending<'_> {
    fn drop(&mut self) {
        self.client.state.borrow_mut().pending.remove(&self.nonce);
    }
}

impl HOWWebSocket {
    pub fn new(host: Rc<dyn TunnelHost>, url: String) -> Result<Rc<Self>, TransportError> {
        if url.is_empty() {
            return Err(TransportError::Host(
                "No HOW websocket endpoint provided".into(),
            ));
        }
        if !crate::how_config::should_simulate_http_over_websocket() {
            TunnelDialOptions::new(host.capabilities())?;
        }
        Ok(Rc::new(Self {
            host,
            url,
            state: RefCell::new(State {
                socket: None,
                time_synced: false,
                auth: HOWWebSocketAuthState::Unknown,
                token_sent: false,
                pending: BTreeMap::new(),
            }),
            preparing: Cell::new(false),
            preparation: RefCell::new(None),
            receiving: Cell::new(false),
            nonce: Cell::new(0),
            disposed: Cell::new(false),
            simulated_closed: Cell::new(false),
            next_ping: Cell::new(0),
            last_ping: Cell::new(None),
            pong_deadline: Cell::new(None),
            tunnel_latency: Cell::new(None),
            monitor: RefCell::new(None),
            generation: Cell::new(0),
            known_latency_domains: RefCell::new(IndexSet::new()),
            used_domains: RefCell::new(IndexSet::new()),
            latency_by_domain: RefCell::new(IndexMap::new()),
            next_latency_ticks: RefCell::new(Vec::new()),
        }))
    }

    pub fn send(&self, data: Value) -> Result<(), TransportError> {
        self.send_retained(native_data::from_json(data)?)
    }

    fn send_retained(&self, data: Input) -> Result<(), TransportError> {
        let message = safe_cex_num::js_retained_json_stringify(&data)
            .map_err(|error| TransportError::Host(error.to_string()))?
            .ok_or_else(|| TransportError::Host("HOW frame serialized as undefined".into()))?;
        let kind = property(&data, "type").and_then(text);
        let socket = self
            .state
            .borrow()
            .socket
            .clone()
            .filter(|socket| socket.ready_state() == 1);
        if let Some(socket) = socket {
            socket.send(SocketFrame::Text(message))?;
        } else {
            // ts-bug: validateAndSend matches ping/pong anywhere in the serialized frame.
            let lower = message.to_ascii_lowercase();
            if !lower.contains("ping")
                && !lower.contains("pong")
                && kind.as_deref() != Some("rpc.ping")
            {
                return Err(TransportError::Host("WebSocketNotReadyError".into()));
            }
        }
        if kind.as_deref() == Some("rpc.http")
            && let Some(url) = property(&data, "arg")
                .and_then(|arg| property(arg, "url"))
                .and_then(text)
        {
            self.track_domain_from_url(&url);
        }
        Ok(())
    }

    pub fn send_token(&self, token: &str) -> Result<(), TransportError> {
        self.state.borrow_mut().token_sent = true;
        self.state.borrow_mut().auth = HOWWebSocketAuthState::Unknown;
        let source = if self.host.capabilities().runtime == crate::socket::SocketRuntime::Browser {
            "frontend"
        } else {
            "worker"
        };
        self.send(json!({"type":"rpc.auth", "token":token, "source":source}))
    }

    pub fn on_message(&self, frame: SocketFrame) -> Result<(), TransportError> {
        // Source checks disposal before decoding or dispatching a message.
        if self.disposed.get() {
            return Ok(());
        }
        if matches!(frame, SocketFrame::Close { .. }) {
            self.on_close();
            return Ok(());
        }
        let Ok((data, binary)) = crate::codec::decode_trade_tunnel_message(frame) else {
            return Ok(());
        };
        match property(&data, "type").and_then(text).as_deref() {
            Some("rpc.auth") => {
                // ts: HOWWebSocket.onMessage uses error truthiness, not status.
                self.state.borrow_mut().auth =
                    if property(&data, "error").is_some_and(safe_cex_num::js_retained_truthy) {
                        HOWWebSocketAuthState::Bad
                    } else {
                        HOWWebSocketAuthState::Good
                    };
            }
            Some("serverTime") => {
                self.state.borrow_mut().time_synced = true;
                if let Some(callback) = crate::how_config::get_how_config().sync_server_time {
                    if let Some(time) = property(&data, "serverTime").and_then(native_data::number)
                    {
                        callback(time);
                    }
                } else if let Some(time) = property(&data, "serverTime")
                    .and_then(native_data::number)
                    .filter(|time| time.is_finite() && *time >= 0.0 && time.fract() == 0.0)
                    .map(|time| time as u64)
                {
                    self.host.sync_server_time(time);
                }
            }
            Some("rpc.ping")
                if property(&data, "status").and_then(text).as_deref() == Some("success") =>
            {
                let now = self.host.now_ms();
                self.pong_deadline.set(Some(now.saturating_add(25_000)));
                if let Some(sent) = self.last_ping.get() {
                    let latency = now.saturating_sub(sent);
                    self.tunnel_latency.set(Some(
                        self.tunnel_latency.get().map_or(latency, |old| {
                            (0.3 * latency as f64 + 0.7 * old as f64).round() as u64
                        }),
                    ));
                }
            }
            Some("heartbeat") => {
                // ts: onMessage catches heartbeat send errors without failing pending requests.
                let _ = self.send(json!({"type":"rpc.ping", "nonce":self.next_nonce()}));
            }
            Some("rpc.latency") => self.handle_latency_response(&native_data::wire_json(&data)?),
            Some("rpc.http") => {
                if let Some(nonce) = property(&data, "nonce").and_then(text) {
                    let (raw, parser) = {
                        let state = self.state.borrow();
                        let Some(slot) = state.pending.get(&nonce) else {
                            return Ok(());
                        };
                        crate::how_diagnostics::log_trade_tunnel_response(
                            &native_data::from_json(slot.request_summary.clone())?,
                            property(&data, "data").unwrap_or(&Input::Undefined),
                            Some(&slot.route),
                        );
                        (slot.raw, slot.parser.clone())
                    };
                    let response = if let Some(error) = property(&data, "data")
                        .and_then(|data| property(data, "error"))
                        .filter(|e| safe_cex_num::js_retained_truthy(e))
                    {
                        Err(TransportError::Host(format!(
                            "TradeTunnelError: {}",
                            crate::how_diagnostics::format_trade_tunnel_error_message(
                                &native_data::wire_json(error)?
                            )
                        )))
                    } else {
                        // Decode typed envelope metadata without rebuffering the native
                        // response payload: Value deserialization can erase lexical -0.
                        let mut response_data =
                            property(&data, "data").cloned().unwrap_or(Input::Undefined);
                        let native_data = native_data::take_property(&mut response_data, "data");
                        decode_response_metadata(&data, &response_data)
                            .map_err(|e| {
                                TransportError::Host(format!("TradeTunnelResponseDecodeError: {e}"))
                            })
                            .and_then(|response| {
                                let body = match if raw {
                                    crate::codec::decode_raw_body(&response.data, binary)?
                                } else {
                                    None
                                } {
                                    Some(raw) => {
                                        if let Some(parser) = &parser {
                                            let raw = String::from_utf8_lossy(&raw);
                                            let value = parser(&raw).map_err(|error| {
                                                TransportError::Host(format!(
                                                    "TradeTunnelResponseDecodeError: {error}"
                                                ))
                                            })?;
                                            crate::RestResponseBody::Transformed(value)
                                        } else {
                                            crate::RestResponseBody::Encoded(raw)
                                        }
                                    }
                                    None => {
                                        let data = native_data.unwrap_or(Input::Undefined);
                                        crate::RestResponseBody::Untransformed(data)
                                    }
                                };
                                let headers: std::collections::HashMap<String, String> =
                                    response.data.headers;
                                Ok(RestResponse {
                                    status: response.data.status,
                                    reason: None,
                                    headers: headers.into_iter().collect(),
                                    body,
                                })
                            })
                    };
                    if let Some(slot) = self.state.borrow_mut().pending.get_mut(&nonce) {
                        slot.result = Some(response);
                    }
                }
            }
            _ => {}
        }
        Ok(())
    }

    pub fn handle_websocket_error(&self, error: &TransportError) -> bool {
        eprintln!("WebSocket error: {error}");
        false
    }

    pub fn start_latency_checks(&self) {
        *self.known_latency_domains.borrow_mut() = crate::how_config::get_unique_latency_domains()
            .into_iter()
            .collect();
        if self.known_latency_domains.borrow().is_empty() {
            return;
        }
        self.schedule_next_latency_tick();
    }

    pub fn stop_latency_checks(&self) {
        self.next_latency_ticks.borrow_mut().pop();
    }

    fn track_domain_from_url(&self, url: &str) {
        let Some(domain) = crate::tunnel_endpoints::extract_domain_from_url(url)
            .filter(|domain| !domain.is_empty())
        else {
            return;
        };
        if self.known_latency_domains.borrow().contains(&domain)
            && self.used_domains.borrow_mut().insert(domain.clone())
        {
            self.query_latency_for_domain(&domain);
        }
    }

    fn query_latency_for_domain(&self, domain: &str) {
        if self.disposed.get() || self.state.borrow().socket.is_none() {
            return;
        }
        let _ =
            self.send(json!({"type":"rpc.latency", "nonce":self.next_nonce(), "domain":domain}));
    }

    fn schedule_next_latency_tick(&self) {
        if self.disposed.get() || self.state.borrow().socket.is_none() {
            return;
        }
        // ts-bug: repeated startLatencyChecks overwrites the handle without cancelling older chains.
        self.next_latency_ticks
            .borrow_mut()
            .push(self.host.now_ms().saturating_add(1_000));
    }

    fn is_latency_stale(&self, domain: &str) -> bool {
        self.latency_by_domain
            .borrow()
            .get(domain)
            .is_none_or(|entry| self.host.now_ms().saturating_sub(entry.updated_at) > 10_000)
    }

    fn query_stale_latencies(&self) {
        if self.disposed.get() || self.state.borrow().socket.is_none() {
            return;
        }
        let domains: Vec<_> = self.used_domains.borrow().iter().cloned().collect();
        for domain in domains {
            if self.is_latency_stale(&domain) {
                self.query_latency_for_domain(&domain);
            }
        }
    }

    fn handle_latency_response(&self, message: &Value) {
        if message.get("error").is_some_and(js_truthy) {
            return;
        }
        let Some(domain) = message["domain"]
            .as_str()
            .filter(|domain| !domain.is_empty())
        else {
            return;
        };
        let Some(latency_ms) = message.get("latency_ms") else {
            return;
        };
        let fallback = |key: &str, default: Value| {
            message
                .get(key)
                .filter(|value| !value.is_null())
                .cloned()
                .unwrap_or(default)
        };
        let entry = LatencyEntry {
            latency_ms: latency_ms.clone(),
            source: message
                .get("source")
                .filter(|value| js_truthy(value))
                .cloned()
                .unwrap_or(json!("unknown")),
            cached: fallback("cached", json!(false)),
            sample_count: fallback("sample_count", json!(1)),
            measured_at: fallback("measured_at", json!(self.host.now_ms())),
            updated_at: self.host.now_ms(),
        };
        self.latency_by_domain
            .borrow_mut()
            .insert(domain.into(), entry);
    }

    pub fn get_latency_for_domain(&self, domain: &str) -> Option<LatencyEntry> {
        self.latency_by_domain.borrow().get(domain).cloned()
    }
    pub fn get_all_latencies(&self) -> IndexMap<String, LatencyEntry> {
        self.latency_by_domain.borrow().clone()
    }

    pub fn on_close(&self) {
        self.generation.set(self.generation.get().wrapping_add(1));
        let mut state = self.state.borrow_mut();
        if let Some(socket) = state.socket.take() {
            let _ = socket.close(None, "");
        }
        state.token_sent = false;
        state.time_synced = false;
        state.auth = HOWWebSocketAuthState::Unknown;
        self.pong_deadline.set(None);
        self.stop_latency_checks();
        for pending in state.pending.values_mut() {
            if pending.result.is_some() {
                continue;
            }
            pending.result = Some(Err(TransportError::Host(
                "TradeTunnel connection lost, reconnecting...".into(),
            )));
        }
    }

    pub fn simulate_close(&self) {
        // ts: ReconnectingWebSocket.close disables automatic reconnection.
        if self.state.borrow().socket.is_some() {
            self.simulated_closed.set(true);
            self.on_close();
        }
    }

    pub fn dispose(&self) {
        self.disposed.set(true);
        let task = self.monitor.borrow_mut().take();
        if let Some(task) = task {
            task.cancel();
        }
        self.generation.set(self.generation.get().wrapping_add(1));
        let socket = {
            let mut state = self.state.borrow_mut();
            state.time_synced = false;
            state.auth = HOWWebSocketAuthState::Unknown;
            state.token_sent = false;
            state.socket.take()
        };
        if let Some(socket) = socket {
            let _ = socket.close(None, "");
        }
        self.stop_latency_checks();
        // TS removes listeners; pending HTTP requests retain their own timeout and error identity.
    }

    pub fn ping(&self) -> Result<(), TransportError> {
        if self.disposed.get() {
            return Ok(());
        }
        self.last_ping.set(Some(self.host.now_ms()));
        self.send(json!({"type":"rpc.ping", "nonce":self.next_nonce()}))?;
        self.next_ping.set(self.host.now_ms().saturating_add(5_000));
        Ok(())
    }

    fn start_monitor(self: &Rc<Self>) {
        if self.monitor.borrow().is_some() {
            return;
        }
        let weak = Rc::downgrade(self);
        let task = self.host.spawn_local(Box::pin(async move {
            loop {
                let Some(client) = weak.upgrade() else {
                    return;
                };
                if client.disposed.get() {
                    return;
                }
                let now = client.host.now_ms();
                let due = {
                    let mut ticks = client.next_latency_ticks.borrow_mut();
                    let due = ticks.iter().filter(|deadline| now >= **deadline).count();
                    ticks.retain(|deadline| now < *deadline);
                    due
                };
                for _ in 0..due {
                    client.query_stale_latencies();
                    client.schedule_next_latency_tick();
                }
                if client.state.borrow().socket.is_none() {
                    let wait = client
                        .host
                        .sleep_until(client.host.now_ms().saturating_add(1_000));
                    drop(client);
                    if wait.await.is_err() {
                        return;
                    }
                    let Some(client) = weak.upgrade() else {
                        return;
                    };
                    if client.preparing.get()
                        || client.simulated_closed.get()
                        || client.disposed.get()
                        || client.state.borrow().socket.is_some()
                    {
                        continue;
                    }
                    let Ok(options) = TunnelDialOptions::new(client.host.capabilities()) else {
                        continue;
                    };
                    client.preparing.set(true);
                    let lease = MonitorLease {
                        client: weak.clone(),
                        preparing: true,
                    };
                    let generation = client.generation.get();
                    let deadline = client.host.now_ms().saturating_add(20_000);
                    let timer = client.host.sleep_until(deadline);
                    let separator = if client.url.contains('?') { '&' } else { '?' };
                    let dial = client.host.connect(
                        format!("{}{separator}v=2&t={}", client.url, client.host.now_ms()),
                        options,
                    );
                    drop(client);
                    let result = race_deadline(timer, dial).await;
                    drop(lease);
                    let Some(client) = weak.upgrade() else {
                        if let Ok(socket) = result {
                            let _ = socket.close(None, "");
                        }
                        return;
                    };
                    if let Ok(socket) = result {
                        if client.disposed.get()
                            || client.generation.get() != generation
                            || client.state.borrow().socket.is_some()
                        {
                            let _ = socket.close(None, "");
                        } else {
                            client.install_socket(socket);
                        }
                    }
                    continue;
                }
                let now = client.host.now_ms();
                if client
                    .pong_deadline
                    .get()
                    .is_some_and(|deadline| now >= deadline)
                {
                    client.on_close();
                    continue;
                }
                if now >= client.next_ping.get() && client.ping().is_err() {
                    client.on_close();
                    continue;
                }
                if client.receiving.get() {
                    let wait = client.host.sleep_until(now.saturating_add(25));
                    drop(client);
                    let _ = wait.await;
                    continue;
                }
                let deadline = client
                    .pong_deadline
                    .get()
                    .unwrap_or(u64::MAX)
                    .min(client.next_ping.get())
                    .min(
                        client
                            .next_latency_ticks
                            .borrow()
                            .iter()
                            .copied()
                            .min()
                            .unwrap_or(u64::MAX),
                    )
                    .min(now.saturating_add(1_000));
                let socket = client
                    .state
                    .borrow()
                    .socket
                    .clone()
                    .expect("socket checked");
                let receive = socket.receive();
                let timer = client.host.sleep_until(deadline);
                let generation = client.generation.get();
                client.receiving.set(true);
                let lease = MonitorLease {
                    client: weak.clone(),
                    preparing: false,
                };
                drop(socket);
                drop(client);
                let frame = race_deadline(timer, receive).await;
                drop(lease);
                let Some(client) = weak.upgrade() else {
                    return;
                };
                if generation != client.generation.get() {
                    continue;
                }
                match frame {
                    Ok(frame) => {
                        let _ = client.on_message(frame);
                    }
                    Err(TransportError::Timeout) => {}
                    Err(error) => {
                        client.handle_websocket_error(&error);
                        client.on_close();
                    }
                }
            }
        }));
        *self.monitor.borrow_mut() = Some(task);
    }

    fn install_socket(&self, socket: Rc<dyn TunnelSocket>) {
        self.next_ping.set(self.host.now_ms().saturating_add(5_000));
        let mut state = self.state.borrow_mut();
        state.socket = Some(socket);
        state.token_sent = false;
        state.time_synced = false;
        state.auth = HOWWebSocketAuthState::Unknown;
        drop(state);
        self.start_latency_checks();
    }

    async fn connect_socket(&self, deadline: u64) -> Result<(), TransportError> {
        let options = TunnelDialOptions::new(self.host.capabilities())?;
        let separator = if self.url.contains('?') { '&' } else { '?' };
        let url = format!("{}{separator}v=2&t={}", self.url, self.host.now_ms());
        let socket = within_deadline(
            self.host.as_ref(),
            deadline,
            self.host.connect(url, options),
        )
        .await?;
        if self.disposed.get() {
            let _ = socket.close(None, "");
            return within_deadline(self.host.as_ref(), deadline, std::future::pending()).await;
        }
        self.install_socket(socket);
        Ok(())
    }

    fn next_nonce(&self) -> String {
        let next = self.nonce.get().wrapping_add(1);
        self.nonce.set(next);
        format!("rust-{next}")
    }

    async fn receive(&self, deadline: u64) -> Result<(), TransportError> {
        if self.disposed.get() {
            return within_deadline(self.host.as_ref(), deadline, std::future::pending()).await;
        }
        let socket = self.state.borrow().socket.clone().ok_or_else(|| {
            TransportError::Host("TradeTunnel connection lost, reconnecting...".into())
        })?;
        if self.receiving.replace(true) {
            within_deadline(
                self.host.as_ref(),
                deadline,
                self.host.sleep_until(self.host.now_ms().saturating_add(25)),
            )
            .await?;
            return Ok(());
        }
        let generation = self.generation.get();
        let _receiving = Receiving(&self.receiving);
        let frame = within_deadline(self.host.as_ref(), deadline, socket.receive()).await;
        if generation != self.generation.get() {
            return Ok(());
        }
        match frame {
            Ok(frame) => self.on_message(frame),
            Err(TransportError::Timeout) => Err(TransportError::Timeout),
            Err(error) => {
                self.on_close();
                Err(error)
            }
        }
    }

    async fn safe_send_token(&self, token: &str, deadline: u64) -> Result<(), TransportError> {
        if self.disposed.get() {
            return within_deadline(self.host.as_ref(), deadline, std::future::pending()).await;
        }
        if token.is_empty() {
            return Ok(());
        }
        for retries in 0..=5 {
            if self.state.borrow().socket.is_none() {
                self.connect_socket(deadline).await?;
            }
            while !self.state.borrow().time_synced {
                self.receive(deadline).await?;
            }
            match self.send_token(token) {
                Err(TransportError::Host(name))
                    if name == "WebSocketNotReadyError" && retries < 5 =>
                {
                    within_deadline(
                        self.host.as_ref(),
                        deadline,
                        self.host
                            .sleep_until(self.host.now_ms().saturating_add(500)),
                    )
                    .await?;
                }
                result => return result,
            }
        }
        unreachable!("last token-send attempt returns its result")
    }

    async fn prepare(self: &Rc<Self>, deadline: u64) -> Result<(), TransportError> {
        while self.preparing.get() {
            let joined = self.preparation.borrow().clone();
            loop {
                within_deadline(
                    self.host.as_ref(),
                    deadline,
                    self.host.sleep_until(self.host.now_ms().saturating_add(25)),
                )
                .await?;
                if let Some(attempt) = &joined {
                    if let Some(outcome) = attempt.outcome.borrow().clone() {
                        outcome?;
                        break;
                    }
                } else if !self.preparing.get() {
                    break;
                }
            }
        }
        self.preparing.set(true);
        let attempt = Rc::new(Preparation {
            outcome: RefCell::new(None),
            share_failure: Cell::new(self.state.borrow().auth == HOWWebSocketAuthState::Bad),
        });
        *self.preparation.borrow_mut() = Some(attempt.clone());
        let _guard = Preparing {
            client: self,
            attempt: attempt.clone(),
        };
        let result = self.prepare_owned(deadline, &attempt.share_failure).await;
        if attempt.share_failure.get()
            && let Err(error) = &result
            && *error != TransportError::Timeout
        {
            *attempt.outcome.borrow_mut() = Some(Err(error.clone()));
        }
        result
    }

    async fn prepare_owned(
        self: &Rc<Self>,
        deadline: u64,
        share_failure: &Cell<bool>,
    ) -> Result<(), TransportError> {
        if self.simulated_closed.get() {
            return within_deadline(
                self.host.as_ref(),
                deadline,
                std::future::pending::<Result<(), TransportError>>(),
            )
            .await;
        }
        if self.state.borrow().socket.is_none() {
            self.connect_socket(deadline).await?;
            self.start_monitor();
        }
        while !self.state.borrow().time_synced {
            self.receive(deadline).await?;
        }
        if self.state.borrow().auth != HOWWebSocketAuthState::Good {
            let mut token = valid_token(self.host.as_ref(), deadline).await?;
            if !self.state.borrow().token_sent && !token.is_empty() {
                self.safe_send_token(&token, deadline).await?;
            }
            let mut retries = 0;
            while self.state.borrow().auth != HOWWebSocketAuthState::Good {
                if self.state.borrow().auth == HOWWebSocketAuthState::Bad {
                    share_failure.set(true);
                    within_deadline(
                        self.host.as_ref(),
                        deadline,
                        self.host
                            .sleep_until(self.host.now_ms().saturating_add(500)),
                    )
                    .await?;
                    if !token.is_empty()
                        && !crate::socket::token_expired(self.host.as_ref(), &token)?
                    {
                        retries += 1;
                    }
                    if retries > 10 {
                        // Only this request exhausted its counter; joined refresh errors remain shared.
                        share_failure.set(false);
                        return Err(TransportError::Host(
                            "Cannot refresh auth token, please reload the terminal".into(),
                        ));
                    }
                    token = valid_token(self.host.as_ref(), deadline).await?;
                    if !token.is_empty() {
                        self.safe_send_token(&token, deadline).await?;
                    }
                }
                self.receive(deadline).await?;
            }
        }
        Ok(())
    }

    async fn request(
        self: &Rc<Self>,
        request: RestRequest,
        raw: bool,
        parser: Option<&JsonParser>,
        route: &Value,
    ) -> Result<RestResponse, TransportError> {
        let deadline = self.host.now_ms().saturating_add(
            request
                .timeout_ms
                .filter(|timeout| *timeout > 0)
                .map_or(20_000, |timeout| timeout as u64),
        );
        if self.disposed.get() {
            return within_deadline(self.host.as_ref(), deadline, std::future::pending()).await;
        }
        self.prepare(deadline).await?;
        if self.host.now_ms() >= deadline {
            return Err(TransportError::Timeout);
        }
        let base = request.base_url.as_deref().unwrap_or("");
        let url = if request.url.starts_with("http") {
            request.url.clone()
        } else {
            format!("{base}{}", request.url)
        };
        let grvt = base.to_ascii_lowercase().contains("grvt.io");
        // ts-bug: createAxiosHttpOverWebsocketAdapter applies || {} to raw config.data, before parsing.
        let data = request
            .body
            .as_ref()
            .map(|body| match body {
                crate::RestBody::Encoded(bytes) => {
                    Input::Text(String::from_utf8_lossy(bytes).encode_utf16().collect())
                }
                crate::RestBody::Untransformed(value) => value.clone(),
                crate::RestBody::Transformed(value) => value.value().clone(),
            })
            .unwrap_or(Input::Undefined);
        let data = if grvt {
            (!matches!(data, Input::Undefined | Input::Null)).then_some(data)
        } else {
            Some(if safe_cex_num::js_retained_truthy(&data) {
                data
            } else {
                Input::Object(Vec::new())
            })
        };
        let data = data
            .map(|data| {
                if let Input::Text(units) = &data {
                    if let Some(parser) = parser {
                        // Custom parsers have an explicit UTF-8 input ABI; native trees do not pass here.
                        let input = String::from_utf16(units)
                            .map_err(|error| TransportError::Host(error.to_string()))?;
                        parser(&input).or_else(|_| parse_fallback_data(&input))
                    } else if raw {
                        let input = String::from_utf16(units)
                            .map_err(|error| TransportError::Host(error.to_string()))?;
                        Ok(big_int_as_string_json_parse(&input))
                    } else {
                        match Input::parse_json(units) {
                            Ok(value) => Ok(value),
                            Err(_) => {
                                let input = String::from_utf16(units)
                                    .map_err(|error| TransportError::Host(error.to_string()))?;
                                parse_fallback_data(&input)
                            }
                        }
                    }
                } else {
                    Ok(data)
                }
            })
            .transpose()?;
        let mut headers: IndexMap<String, Input> = request
            .headers
            .into_iter()
            .map(|(key, value)| (key, Input::Text(value.encode_utf16().collect())))
            .collect();
        // Only HOW interprets these native GRVT config extensions.
        for (property, header) in [
            ("grvtCookie", "Cookie"),
            ("grvtAccountId", "X-Grvt-Account-Id"),
        ] {
            if let Some(value) = request
                .config_extensions
                .get(property)
                .filter(|value| safe_cex_num::js_retained_truthy(value))
            {
                headers.insert(header.into(), value.clone());
            }
        }
        if request.params.is_some() && request.query_params.is_some() {
            return Err(TransportError::Host(
                "Cannot combine legacy and native query parameters".into(),
            ));
        }
        let mut params: serde_json::Map<String, Value> = match request.query_params.as_ref() {
            None => match request.params.as_ref() {
                None | Some(Value::Null) => serde_json::Map::new(),
                Some(Value::Object(params)) => params.clone(),
                Some(value) if !safe_cex_num::js_truthy(Some(value)) => serde_json::Map::new(),
                Some(_) => {
                    return Err(TransportError::Host(
                        "HOW query params must be an object".into(),
                    ));
                }
            },
            Some(value @ safe_cex_types::QueryValue::Object(_)) => {
                match crate::query::query_json_projection(value) {
                    Some(Value::Object(params)) => params,
                    _ => unreachable!("object query JSON projection"),
                }
            }
            Some(value) if !crate::query::query_truthy(value) => serde_json::Map::new(),
            Some(_) => {
                return Err(TransportError::Host(
                    "HOW query params must be an object".into(),
                ));
            }
        };
        for value in params.values_mut() {
            normalize_json_numbers(value, false, false);
        }
        normalize_object_order(&mut params);
        let method = request.method.to_uppercase();
        let summary = json!({"url":url,"method":method});
        // The shared protocol record supplies only finite envelope metadata.
        // Native header overrides and data are attached outside its Value decoder.
        let args = HttpRequestArgs::<Value> {
            headers: serde_json::Map::new(),
            method,
            url,
            data: None,
            params,
        };
        let mut args = native_data::from_json(
            serde_json::to_value(args).map_err(|error| TransportError::Host(error.to_string()))?,
        )?;
        native_data::take_property(&mut args, "headers");
        let Input::Object(fields) = &mut args else {
            unreachable!("request args object")
        };
        fields.insert(
            0,
            (
                "headers".encode_utf16().collect(),
                native_data::object(headers),
            ),
        );
        if let Some(data) = data {
            fields.insert(3, ("data".encode_utf16().collect(), data));
        }
        let nonce = self.next_nonce();
        let guard = Pending {
            client: self,
            nonce: nonce.clone(),
        };
        self.state.borrow_mut().pending.insert(
            nonce.clone(),
            PendingResponse {
                raw,
                parser: parser.cloned(),
                request_summary: summary,
                route: route.clone(),
                result: None,
            },
        );
        let mut frame = json!({"type":"rpc.http", "rpc_version":4});
        if raw {
            frame["format"] = json!("raw");
            frame["accept_encoding"] = json!(["gzip"]);
            frame["accept_binary"] = json!(true);
        }
        if base.to_ascii_lowercase().contains("grvt") {
            frame["bypass_proxy"] = json!(true);
        }
        frame["nonce"] = json!(nonce);
        crate::how_diagnostics::log_trade_tunnel_request(&args, Some(route));
        let mut frame = native_data::from_json(frame)?;
        let Input::Object(fields) = &mut frame else {
            unreachable!("frame object")
        };
        fields.push(("arg".encode_utf16().collect(), args));
        if self.host.now_ms() >= deadline {
            return Err(TransportError::Timeout);
        }
        self.send_retained(frame)?;
        loop {
            if let Some(response) = self
                .state
                .borrow_mut()
                .pending
                .get_mut(&guard.nonce)
                .and_then(|slot| slot.result.take())
            {
                return response;
            }
            self.receive(deadline).await?;
        }
    }
}

impl Drop for HOWWebSocket {
    fn drop(&mut self) {
        self.dispose();
    }
}

fn decode_response_metadata(
    frame: &Input,
    response: &Input,
) -> Result<RpcHttpResponse, serde_json::Error> {
    let selected = native_data::object(
        [
            "config",
            "data_raw",
            "data_encoded",
            "data_encoding",
            "status",
            "status_text",
            "headers",
        ]
        .into_iter()
        .filter_map(|name| property(response, name).cloned().map(|value| (name, value))),
    );
    let mut envelope = native_data::object([
        (
            "type",
            property(frame, "type").cloned().unwrap_or(Input::Undefined),
        ),
        (
            "nonce",
            property(frame, "nonce")
                .cloned()
                .unwrap_or(Input::Undefined),
        ),
        ("data", selected),
    ])
    .into_value()?;
    normalize_json_numbers(&mut envelope, false, false);
    serde_json::from_value(envelope)
}

fn js_truthy(value: &Value) -> bool {
    safe_cex_num::js_truthy(Some(value))
}

// ts: JSON.stringify enumerates canonical array-index keys first, then other keys in insertion order.
fn normalize_object_order(values: &mut serde_json::Map<String, Value>) {
    let mut indexed = Vec::new();
    let mut other = Vec::new();
    for (key, value) in std::mem::take(values) {
        let index = key
            .parse::<u32>()
            .ok()
            .filter(|index| *index != u32::MAX && index.to_string() == key);
        if let Some(index) = index {
            indexed.push((index, key, value));
        } else {
            other.push((key, value));
        }
    }
    indexed.sort_by_key(|(index, _, _)| *index);
    values.extend(indexed.into_iter().map(|(_, key, value)| (key, value)));
    values.extend(other);
}

pub(crate) fn normalize_json_numbers(value: &mut Value, raw: bool, nested: bool) {
    match value {
        Value::Number(number) => {
            let text = number.to_string();
            // Match bigIntAsStringJsonParse's nested integer protection, including its 16-digit threshold.
            if raw
                && nested
                && text.trim_start_matches('-').len() >= 16
                && text
                    .trim_start_matches('-')
                    .bytes()
                    .all(|byte| byte.is_ascii_digit())
            {
                *value = Value::String(text);
            } else if let Some(number) = number.as_f64().filter(|number| number.is_finite()) {
                // Number-to-string erases -0; retain its lexeme until wire projection.
                if number == 0.0 && number.is_sign_negative() {
                    return;
                }
                *value = serde_json::from_str(&safe_cex_num::js_number_string(number))
                    .unwrap_or(Value::Null);
            }
            // JSON.parse retains overflow as +/-Infinity until a later serialization.
            // Keep its arbitrary-precision source Number for shared JS coercions;
            // projecting to null here changes String/Boolean venue branches.
        }
        Value::Array(values) => {
            for value in values {
                normalize_json_numbers(value, raw, true);
            }
        }
        Value::Object(values) => {
            for value in values.values_mut() {
                normalize_json_numbers(value, raw, true);
            }
            normalize_object_order(values);
        }
        _ => {}
    }
}

// ts-bug: bigIntAsStringJsonParse rewrites digit runs even inside quoted strings;
// corruption makes JSON.parse fail and the outbound HOW body becomes null.
fn protect_big_integer_tokens(text: &[u16]) -> Vec<u16> {
    let mut output = Vec::with_capacity(text.len());
    let mut index = 0;
    while index < text.len() {
        if matches!(text[index], 58 | 44 | 91 | 93) {
            let mut number = index + 1;
            while text.get(number).is_some_and(|unit| {
                let mut utf8 = [0; 4];
                char::from_u32((*unit).into()).is_some_and(|scalar| {
                    safe_cex_num::js_trim_start(scalar.encode_utf8(&mut utf8)).is_empty()
                })
            }) {
                number += 1;
            }
            let digits = number + usize::from(text.get(number) == Some(&45));
            let mut end = digits;
            while text.get(end).is_some_and(|unit| matches!(unit, 48..=57)) {
                end += 1;
            }
            if end - digits >= 16
                && !text
                    .get(end)
                    .is_some_and(|unit| matches!(unit, 48..=57 | 46 | 34 | 101 | 69))
            {
                output.extend_from_slice(&text[index..number]);
                output.push(34);
                output.extend_from_slice(&text[number..end]);
                output.push(34);
                index = end;
                continue;
            }
        }
        output.push(text[index]);
        index += 1;
    }
    output
}

/// Parse JSON with JavaScript number normalization, returning null on invalid JSON.
pub fn json_parse(text: &str) -> Input {
    native_data::parse(text).unwrap_or(Input::Null)
}

/// Preserve the shared TS parser's rewrite, parse, and null-on-failure contract.
pub fn big_int_as_string_json_parse(text: &str) -> Input {
    big_int_as_string_json_parse_utf16(&text.encode_utf16().collect::<Vec<_>>())
}

/// Native string entry; the existing UTF-8 helper delegates to this one rewrite/parser.
pub fn big_int_as_string_json_parse_utf16(text: &[u16]) -> Input {
    // ts-bug: the source regex also corrupts matching runs inside quoted strings.
    Input::parse_json(&protect_big_integer_tokens(text)).unwrap_or(Input::Null)
}

#[cfg(test)]
fn parse_data(bytes: &[u8], raw: bool) -> Result<Input, TransportError> {
    let text = std::str::from_utf8(bytes).map_err(|e| TransportError::Host(e.to_string()))?;
    if raw {
        return Ok(big_int_as_string_json_parse(text));
    }
    native_data::parse(text).or_else(|_| parse_fallback_data(text))
}

fn parse_fallback_data(text: &str) -> Result<Input, TransportError> {
    let fields: Vec<_> = text.split('&').collect();
    if fields.iter().all(|field| {
        field.split_once('=').is_some_and(|(key, value)| {
            !key.is_empty() && !value.is_empty() && !value.contains('=')
        })
    }) {
        let mut data = IndexMap::new();
        for field in fields {
            let (key, value) = field.split_once('=').expect("validated form field");
            let key = crate::relay::decode_query(key);
            // Source assigns strings into {}, invoking its primitive prototype setter.
            if key != "__proto__" {
                data.insert(
                    key,
                    Input::Text(crate::relay::decode_query(value).encode_utf16().collect()),
                );
            }
        }
        Ok(native_data::object(data))
    } else {
        Ok(Input::Text(text.encode_utf16().collect()))
    }
}

struct RegionalClients {
    host: Weak<dyn TunnelHost>,
    jp: Weak<HOWWebSocket>,
    sg: Weak<HOWWebSocket>,
}
fn with_regional_clients<T>(callback: impl FnOnce(&RefCell<Vec<RegionalClients>>) -> T) -> T {
    thread_local! {
        static REGIONAL_CLIENTS: RefCell<Vec<RegionalClients>> = const { RefCell::new(Vec::new()) };
    }
    REGIONAL_CLIENTS.with(callback)
}

fn regional_clients(
    host: Rc<dyn TunnelHost>,
) -> Result<(Rc<HOWWebSocket>, Rc<HOWWebSocket>), TransportError> {
    with_regional_clients(|clients| {
        let mut clients = clients.borrow_mut();
        clients.retain(|entry| entry.jp.strong_count() > 0 || entry.sg.strong_count() > 0);
        let entry = clients
            .iter_mut()
            .find(|entry| Weak::ptr_eq(&entry.host, &Rc::downgrade(&host)));
        if let Some(entry) = entry {
            let jp = entry.jp.upgrade().map(Ok).unwrap_or_else(|| {
                HOWWebSocket::new(host.clone(), tunnel_endpoint(HowEgress::Jp).into())
            })?;
            let sg = entry.sg.upgrade().map(Ok).unwrap_or_else(|| {
                HOWWebSocket::new(host.clone(), tunnel_endpoint(HowEgress::Sg).into())
            })?;
            entry.jp = Rc::downgrade(&jp);
            entry.sg = Rc::downgrade(&sg);
            return Ok((jp, sg));
        }
        let jp = HOWWebSocket::new(host.clone(), tunnel_endpoint(HowEgress::Jp).into())?;
        let sg = HOWWebSocket::new(host.clone(), tunnel_endpoint(HowEgress::Sg).into())?;
        clients.push(RegionalClients {
            host: Rc::downgrade(&host),
            jp: Rc::downgrade(&jp),
            sg: Rc::downgrade(&sg),
        });
        Ok((jp, sg))
    })
}

pub fn get_how_websocket_for_exchange(
    host: Rc<dyn TunnelHost>,
    exchange: &str,
) -> Result<Rc<HOWWebSocket>, TransportError> {
    let (jp, sg) = regional_clients(host)?;
    Ok(if is_tokyo_egress(exchange) { jp } else { sg })
}

pub fn get_latency_for_domain(
    host: Rc<dyn TunnelHost>,
    exchange: &str,
    domain: &str,
) -> Result<Option<LatencyEntry>, TransportError> {
    Ok(get_how_websocket_for_exchange(host, exchange)?.get_latency_for_domain(domain))
}

type RegionalLatencies = (
    IndexMap<String, LatencyEntry>,
    IndexMap<String, LatencyEntry>,
);
pub fn get_all_server_latencies(
    host: Rc<dyn TunnelHost>,
) -> Result<RegionalLatencies, TransportError> {
    let (jp, sg) = regional_clients(host)?;
    Ok((sg.get_all_latencies(), jp.get_all_latencies()))
}

pub fn get_tunnel_latency_for_exchange(
    host: Rc<dyn TunnelHost>,
    exchange: &str,
) -> Result<Option<u64>, TransportError> {
    Ok(get_how_websocket_for_exchange(host, exchange)?
        .tunnel_latency
        .get())
}

pub fn get_all_tunnel_latencies(
    host: Rc<dyn TunnelHost>,
) -> Result<(Option<u64>, Option<u64>), TransportError> {
    let (jp, sg) = regional_clients(host)?;
    Ok((sg.tunnel_latency.get(), jp.tunnel_latency.get()))
}

pub fn dispose_how_websockets() {
    let live = with_regional_clients(|clients| {
        clients
            .borrow()
            .iter()
            .flat_map(|entry| [entry.jp.upgrade(), entry.sg.upgrade()])
            .flatten()
            .collect::<Vec<_>>()
    });
    for client in live {
        client.dispose();
    }
}

pub struct HowRestAdapter {
    jp: Rc<HOWWebSocket>,
    sg: Rc<HOWWebSocket>,
    region: Option<HowEgress>,
    raw: bool,
    custom_parser: Option<JsonParser>,
}
impl HowRestAdapter {
    pub fn new(
        host: Rc<dyn TunnelHost>,
        region: Option<HowEgress>,
        raw: bool,
    ) -> Result<Self, TransportError> {
        let (jp, sg) = regional_clients(host)?;
        Ok(Self {
            jp,
            sg,
            region,
            raw,
            custom_parser: None,
        })
    }
    pub fn with_options(
        host: Rc<dyn TunnelHost>,
        options: CreateAxiosHttpOverWebsocketAdapterOptions,
    ) -> Result<Self, TransportError> {
        let mut adapter = Self::new(
            host,
            options.region.map(Into::into),
            options.custom_parser.is_some(),
        )?;
        adapter.custom_parser = options.custom_parser;
        Ok(adapter)
    }

    fn selected_region(&self, request: &RestRequest) -> HowEgress {
        self.region.unwrap_or_else(|| {
            if is_tokyo_egress(request.base_url.as_deref().unwrap_or("")) {
                HowEgress::Jp
            } else {
                HowEgress::Sg
            }
        })
    }

    fn route_context(&self, request: &RestRequest) -> Value {
        let region = self.selected_region(request);
        let mut route = json!({"selectedRegion":if region == HowEgress::Jp {"JA"} else {"SG"}, "tunnelHost":if region == HowEgress::Jp {"tokyo"} else {"singapore"}});
        if let Some(forced) = self.region {
            route["forcedRegion"] = json!(if forced == HowEgress::Jp { "JA" } else { "SG" });
        }
        if crate::how_config::should_simulate_http_over_websocket() {
            route["simulated"] = json!(true);
        } else {
            route["bypassProxy"] = json!(
                request
                    .base_url
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains("grvt")
            );
        }
        route
    }

    pub fn execute_with_context(&self, request: RestRequest) -> ContextFuture<'_> {
        Box::pin(async move {
            let route = self.route_context(&request);
            let method = request.method.to_uppercase();
            let url = request_url(&request);
            match self.execute(request.clone()).await {
                Ok(response) => Ok(HOWResponse {
                    response,
                    config: request,
                }),
                Err(error)
                    if matches!(
                        error.transport_error.as_deref(),
                        Some(TransportError::Timeout)
                    ) =>
                {
                    Err(HOWRequestError::Timeout(Box::new(
                        TradeTunnelTimeoutError::new(
                            format!("WSoverHTTP request timed out: {}", request.url),
                            request,
                            method,
                            route,
                            url,
                        ),
                    )))
                }
                Err(error) => Err(HOWRequestError::Transport {
                    error: Box::new(error),
                    config: Box::new(request),
                    route,
                    method,
                    url,
                }),
            }
        })
    }
}
type ContextFuture<'a> =
    std::pin::Pin<Box<dyn std::future::Future<Output = Result<HOWResponse, HOWRequestError>> + 'a>>;
fn request_url(request: &RestRequest) -> String {
    if request.url.starts_with("http") {
        request.url.clone()
    } else {
        format!(
            "{}{}",
            request.base_url.as_deref().unwrap_or(""),
            request.url
        )
    }
}

impl RestAdapter for HowRestAdapter {
    fn query_in_url(&self) -> bool {
        false
    }
    fn execute(&self, request: RestRequest) -> crate::RestFuture<'_, RestResponse> {
        Box::pin(async move {
            let original = request.clone();
            let failure = |error| crate::RestFailure::from_transport(error, original.clone());
            let client = match self.selected_region(&request) {
                HowEgress::Jp => &self.jp,
                HowEgress::Sg => &self.sg,
            };
            let route = self.route_context(&request);
            if crate::how_config::should_simulate_http_over_websocket() {
                let config = crate::how_config::get_how_config();
                let adapter = config.direct_adapter.ok_or_else(|| {
                    failure(TransportError::Host(
                        "HOW simulation requires a direct HTTP adapter".into(),
                    ))
                })?;
                let deadline = client.host.now_ms().saturating_add(
                    request
                        .timeout_ms
                        .filter(|value| *value > 0)
                        .map_or(20_000, |value| value as u64),
                );
                let mut request = request;
                request.body = request
                    .body
                    .as_ref()
                    .map(crate::RestBody::wire_bytes)
                    .transpose()
                    .map_err(failure)?
                    .map(crate::RestBody::Encoded);
                request.url = request_url(&request);
                request.method = request.method.to_uppercase();
                if !request
                    .base_url
                    .as_deref()
                    .unwrap_or("")
                    .to_ascii_lowercase()
                    .contains("grvt.io")
                    && request.body.as_ref().is_none_or(
                        |body| matches!(body, crate::RestBody::Encoded(bytes) if bytes.is_empty()),
                    )
                {
                    request.body = Some(crate::RestBody::Encoded(b"{}".to_vec()));
                }
                let body = request
                    .body
                    .as_ref()
                    .map(|body| match body {
                        crate::RestBody::Encoded(bytes) => {
                            Input::Text(String::from_utf8_lossy(bytes).encode_utf16().collect())
                        }
                        crate::RestBody::Untransformed(value) => value.clone(),
                        crate::RestBody::Transformed(data) => data.value().clone(),
                    })
                    .unwrap_or(Input::Undefined);
                let mut logged = native_data::from_json(json!({"url":request.url,"method":request.method,"params":request.query_params.as_ref().and_then(crate::query::query_json_projection).or_else(|| request.params.clone())})).map_err(failure)?;
                let Input::Object(fields) = &mut logged else {
                    unreachable!("request log object")
                };
                fields.push(("data".encode_utf16().collect(), body));
                crate::how_diagnostics::log_trade_tunnel_request(&logged, Some(&route));
                let response = within_deadline(client.host.as_ref(), deadline, async {
                    Ok(adapter.execute(request).await)
                })
                .await
                .map_err(failure)??;
                let body = native_data::object([("data", response.body.clone().into_data())]);
                crate::how_diagnostics::log_trade_tunnel_response(&logged, &body, Some(&route));
                Ok(response)
            } else {
                client
                    .request(request, self.raw, self.custom_parser.as_ref(), &route)
                    .await
                    .map_err(failure)
            }
        })
    }
}

#[cfg(test)]
mod wire_tests {
    use super::*;
    fn json_parse(text: &str) -> Value {
        crate::RestResponseBody::Transformed(super::json_parse(text))
            .json_value()
            .unwrap()
    }
    fn big_int_as_string_json_parse(text: &str) -> Value {
        crate::RestResponseBody::Transformed(super::big_int_as_string_json_parse(text))
            .json_value()
            .unwrap()
    }
    fn parse_data(bytes: &[u8], raw: bool) -> Result<Value, TransportError> {
        super::parse_data(bytes, raw).map(|value| {
            crate::RestResponseBody::Transformed(value)
                .json_value()
                .unwrap()
        })
    }

    #[test]
    fn behavior_native_form_fallback_preserves_setter_effect() {
        let cases = [
            ("__proto__=kept&ok=1", json!({"ok":"1"})),
            ("%5F%5Fproto%5F%5F=kept&ok=1", json!({"ok":"1"})),
            ("__proto__=kept&__proto__=again&ok=1", json!({"ok":"1"})),
            (
                "constructor=kept&toString=own",
                json!({"constructor":"kept","toString":"own"}),
            ),
            ("ok=1&ok=2", json!({"ok":"2"})),
            (
                r#"{"__proto__":"kept","ok":1}"#,
                json!({"__proto__":"kept","ok":1}),
            ),
            ("raw", json!("raw")),
            ("%5f%5fproto%5f%5f=x&a=y", json!({"a":"y"})),
        ];
        for (input, expected) in &cases {
            let actual = parse_data(input.as_bytes(), false).unwrap();
            assert_eq!(&actual, expected, "HOW FORM native decoded data: {input}");
        }
        let output = std::process::Command::new("node")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/form-behavior-source.cjs"
            ))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "HOW FORM actual source: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let actual: Vec<Value> = serde_json::from_slice(&output.stdout).unwrap();
        assert_eq!(actual.len(), 8, "HOW FORM independent source case floor");
        for i in 0..8 {
            assert_eq!(actual[i], cases[i].1, "HOW FORM source case {i}");
        }
    }

    #[test]
    fn finding_request_dump_body_rebuffers_signed_zero() {
        let frame: Value = serde_json::from_str(r#"{"type":"rpc.http","nonce":"fixture","data":{"config":{"url":"/fixture"},"status":200,"status_text":"OK","headers":{},"request_dump":{"method":"GET","url":"/fixture","headers":{},"body":{"n":-0}}}}"#).unwrap();
        assert_eq!(
            frame["data"]["request_dump"]["body"]["n"]
                .as_f64()
                .unwrap()
                .to_bits(),
            (-0_f64).to_bits(),
            "diagnostic input retains negative zero"
        );
        let decoded: RpcHttpResponse = serde_json::from_value(frame).unwrap();
        let dump = decoded.data.request_dump.unwrap();
        assert_eq!(
            dump.body.unwrap()["n"].as_f64().unwrap().to_bits(),
            0_f64.to_bits(),
            "FINDING shared RequestDump.body metadata still rebuffers lexical negative zero; flip when repaired"
        );
    }

    #[test]
    fn raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace() {
        let script = r#"
const load = require('./sdk-rs/safe-cex-transport/scripts/ts-source.cjs');
const { bigIntAsStringJsonParse: parse, jsonParse } = load('./packages/json-parse/src/jsonParse.ts', source => {
 if (process.env.P2B_JSON_PARSE_CORRECTION_CONTROL !== '1') return source;
 const original = `safeStr = str.replace(BIG_INT_REPLACE_REGEX, '$1"$2"');`;
 require('node:assert/strict').equal(source.split(original).length, 2, 'correction mutation must hit actual TS');
 return source.replace(original, 'safeStr = str;');
});
const inputs = [
 '{"text":"x:1234567890123456,"}',
 '{"text":"x:123456789012345,"}',
 '{"text":"x,-1234567890123456!"}',
 '{"text":"é[1234567890123456]"}',
 '{"text":"x:1234567890123456e1,"}',
 '{"text":"x:1234567890123456.5,"}',
 '{"text":"x:1234567890123456"}',
 '{"text":"x:\uFEFF1234567890123456,"}',
 '{"text":"x:\u00851234567890123456,"}',
 '{"a":[1234567890123456,-1234567890123456,1234567890123456.5,1234567890123456e1]}'
];
const whitespace = [];
for (let n=0;n<=0x10ffff;n++) {
 if (n>=0xd800 && n<=0xdfff) continue;
 if (String.fromCodePoint(n).trim()==='') whitespace.push(n);
}
const cases = inputs.map(input=>({input,result:parse(input),plain:jsonParse(input)}));
const control = process.env.P2B_JSON_PARSE_CARDINALITY_CONTROL;
if (control) {
 const assert = require('node:assert/strict');
 assert.equal(cases.length, 10, 'HOW-PARSER original source corpus');
 if (control === 'empty') cases.splice(0);
 else if (control === 'subset') cases.splice(9);
 else if (control === 'extra') cases.push(cases[0]);
 else throw new Error(`unknown cardinality control ${control}`);
 assert.equal(cases.length, {empty:0, subset:9, extra:11}[control], 'HOW-PARSER mutation applied');
 process.stderr.write(`HOW-PARSER mutation applied ${control} ${cases.length}\n`);
}
process.stdout.write(JSON.stringify({cases,whitespace}));
"#;
        let output = std::process::Command::new("node")
            .args(["-e", script])
            .current_dir(concat!(env!("CARGO_MANIFEST_DIR"), "/../.."))
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        if let Ok(control) = std::env::var("P2B_JSON_PARSE_CARDINALITY_CONTROL") {
            let marker = format!("HOW-PARSER mutation applied {control} ");
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert!(
                stderr.contains(&marker),
                "capture mutation must execute: {stderr}"
            );
            eprint!("{stderr}");
        }
        let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
        let cases = oracle["cases"].as_array().unwrap();
        assert_eq!(cases.len(), 10, "HOW-PARSER source case count");
        let mut comparisons = 0usize;
        // ts-bug: correction-specific witness; an upstream regex fix must force a revisit.
        assert_eq!(
            oracle["cases"].as_array().unwrap().len(),
            10,
            "HOW-PARSER-CARDINALITY source cases"
        );
        assert_eq!(oracle["cases"][0]["result"], Value::Null);
        for index in 0..10 {
            let case = cases
                .get(index)
                .expect("HOW-PARSER missing independently required case");
            let input = case["input"].as_str().unwrap();
            assert_eq!(
                parse_data(input.as_bytes(), true).unwrap(),
                case["result"],
                "{input}"
            );
            comparisons += 1;
            assert_eq!(
                big_int_as_string_json_parse(input),
                case["result"],
                "{input}"
            );
            comparisons += 1;
            assert_eq!(json_parse(input), case["plain"], "{input}");
            comparisons += 1;
        }
        assert_eq!(comparisons, 30, "HOW-PARSER native comparison count");
        assert_eq!(
            cases.len() * 3,
            comparisons,
            "HOW-PARSER source/native case coverage"
        );
        let whitespace: Vec<u32> = (0..=0x10ffff)
            .filter(|n| {
                char::from_u32(*n).is_some_and(|ch| {
                    let mut encoded = [0; 4];
                    safe_cex_num::js_trim(ch.encode_utf8(&mut encoded)).is_empty()
                })
            })
            .collect();
        assert_eq!(whitespace.len(), 25, "HOW-PARSER native whitespace count");
        assert_eq!(
            oracle["whitespace"].as_array().unwrap().len(),
            25,
            "HOW-PARSER source whitespace count"
        );
        assert_eq!(
            serde_json::to_value(whitespace).unwrap(),
            oracle["whitespace"]
        );
    }

    #[test]
    fn raw_parser_ci_rejects_empty_subset_and_extra_source_cases() {
        for (control, count) in [("empty", 0), ("subset", 9), ("extra", 11)] {
            let output = std::process::Command::new(std::env::current_exe().unwrap())
                .args(["--exact", "how::wire_tests::raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace", "--nocapture"])
                .env_remove("P2B_JSON_PARSE_CORRECTION_CONTROL")
                .env("P2B_JSON_PARSE_CARDINALITY_CONTROL", control)
                .output().unwrap();
            let stdout = String::from_utf8_lossy(&output.stdout);
            let stderr = String::from_utf8_lossy(&output.stderr);
            assert_eq!(
                output.status.code(),
                Some(101),
                "{control}: {stdout}\n{stderr}"
            );
            assert!(
                stderr.contains(&format!("HOW-PARSER mutation applied {control} {count}")),
                "{stderr}"
            );
            assert!(
                stderr.contains("HOW-PARSER source case count"),
                "{stdout}\n{stderr}"
            );
            assert!(stdout.contains("1 failed"), "{stdout}\n{stderr}");
        }
    }

    #[test]
    fn raw_parser_ci_rejects_live_typescript_correction() {
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .args(["--exact", "how::wire_tests::raw_parser_pins_live_typescript_corruption_and_ecmascript_whitespace", "--nocapture"])
            .env_remove("P2B_JSON_PARSE_CARDINALITY_CONTROL")
            .env("P2B_JSON_PARSE_CORRECTION_CONTROL", "1")
            .output().unwrap();
        let stdout = String::from_utf8_lossy(&output.stdout);
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert_eq!(output.status.code(), Some(101), "{stdout}\n{stderr}");
        assert!(stdout.contains("1 failed"), "{stdout}\n{stderr}");
        assert!(
            stderr.contains("assertion `left == right` failed"),
            "{stdout}\n{stderr}"
        );
        assert!(!stderr.contains("MODULE_NOT_FOUND"), "{stderr}");
    }

    #[test]
    fn raw_integer_scanner_protects_exactly_nested_sixteen_digit_tokens() {
        let input = r#"{"short":123456789012345,"long":1234567890123456,"negative":-1234567890123456,"array":[123456789012345,1234567890123456],"decimal":1234567890123456.5,"exp":1234567890123456e1,"text":"1234567890123456"}"#;
        let expected = r#"{"short":123456789012345,"long":"1234567890123456","negative":"-1234567890123456","array":[123456789012345,"1234567890123456"],"decimal":1234567890123456.5,"exp":1234567890123456e1,"text":"1234567890123456"}"#;
        assert_eq!(
            protect_big_integer_tokens(&input.encode_utf16().collect::<Vec<_>>()),
            expected.encode_utf16().collect::<Vec<_>>()
        );
        assert_eq!(
            protect_big_integer_tokens(&"1234567890123456".encode_utf16().collect::<Vec<_>>()),
            "1234567890123456".encode_utf16().collect::<Vec<_>>()
        );
    }

    #[test]
    fn shared_request_schema_roundtrips_maps_and_params_but_server_refuses_ts_batch_bodies() {
        let mut args = HttpRequestArgs::<Value> {
            headers: serde_json::Map::new(),
            method: "POST".into(),
            url: "https://venue/orders".into(),
            data: Some(json!({"qty":"1"})),
            params: serde_json::from_value(json!({"symbol":"BTC","ids":[1,2]})).unwrap(),
        };
        let wire = serde_json::to_value(&args).unwrap();
        let server: HttpRequestArgs = serde_json::from_value(wire).unwrap();
        assert_eq!(server.data.unwrap()["qty"], "1");
        assert_eq!(server.params["ids"], json!([1, 2]));
        // ts-bug: parseWebsocketData permits batch arrays; the server map-only data field rejects them.
        args.data = Some(json!([{"qty":"1"}]));
        let wire = serde_json::to_value(&args).unwrap();
        assert_eq!(wire["data"], json!([{"qty":"1"}]));
        assert!(serde_json::from_value::<HttpRequestArgs>(wire).is_err());
    }
}

#[cfg(test)]
mod json_configuration_tests {
    #[test]
    fn shared_frames_use_identical_json_dependency_configuration() {
        let sdk: toml::Value = toml::from_str(include_str!("../../Cargo.toml")).unwrap();
        let server: toml::Value =
            toml::from_str(include_str!("../../../infra/tradetunnel/Cargo.toml")).unwrap();
        let sdk = &sdk["workspace"]["dependencies"]["serde_json"];
        let server = &server["dependencies"]["serde_json"];
        assert_eq!(
            sdk, server,
            "path-shared Map and Number must have identical dependency semantics"
        );
        assert_eq!(sdk["version"].as_str(), Some("=1.0.152"));
        for feature in ["preserve_order", "float_roundtrip", "arbitrary_precision"] {
            assert!(
                sdk["features"]
                    .as_array()
                    .unwrap()
                    .iter()
                    .any(|item| item.as_str() == Some(feature))
            );
        }
    }
}

#[cfg(test)]
#[path = "../tests/shared_truthiness/mod.rs"]
mod shared_truthiness;
