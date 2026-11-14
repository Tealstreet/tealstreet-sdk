//! Where a host's sockets and requests go: the network, or a recording.
use crate::routing::{ProxyTokenProvider, RoutingPlan, RoutingState, authenticate_relay};
use crate::{Events, HostEvent};
use safe_cex_engine::{HttpRequest, SocketId};
#[cfg(feature = "nonce-drain")]
use std::future::Future;
use std::io::Read;
#[cfg(feature = "nonce-drain")]
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    handshake::client::Request,
    http::{HeaderName, HeaderValue},
};

mod http;
mod websocket;
pub use http::{BoundHttp, HttpPrepareError, HttpPrepareFuture, PreparedHttp};

/// Run admission unlocked, then revalidate the socket and commit around start_send.
/// Refused handoffs stay NotSent; accepted frames never enter an async staging queue.
/// Uncertain send/flush failures report HttpFailed after release commit.
pub trait WebSocketWriter: Send + Sync {
    fn try_write(
        &self,
        request: safe_cex_engine::RequestId,
        claim: &mut dyn FnMut(&mut dyn safe_cex_engine::WebSocketHandoff),
    ) -> Result<(), WebSocketWriteError>;

    /// Revoke this connection's typed handoff when its host closes it.
    fn retire(&self);
}

#[derive(Debug)]
pub enum WebSocketWriteError {
    NotReady,
    Send(String),
}

/// Control traffic retains its existing queue. Mutation-capable transports
/// additionally supply a connection-owned physical writer; the default is None.
pub struct SocketConnection {
    pub outgoing: mpsc::UnboundedSender<String>,
    pub writer: Option<Arc<dyn WebSocketWriter>>,
}

#[cfg(feature = "nonce-drain")]
pub struct Drained {
    _sealed: (),
}
#[cfg(feature = "nonce-drain")]
impl std::fmt::Debug for Drained {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Drained")
    }
}
#[cfg(feature = "nonce-drain")]
pub type DrainFuture = Pin<Box<dyn Future<Output = Drained> + Send>>;

pub trait Transport {
    /// Seal new work and join owned resources. Unsupported transports cannot
    /// produce retirement evidence; ordinary transport use remains available.
    #[cfg(feature = "nonce-drain")]
    fn drain(&self) -> Option<DrainFuture> {
        None
    }
    /// Managed requests require explicit route preparation. The default fails
    /// closed; a custom transport must implement the same final-handoff seam.
    fn prepare_http(
        &self,
        _request: HttpRequest,
        _intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        Box::pin(async { Err(HttpPrepareError::Unsupported) })
    }
    fn prepare_http_classified(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
        _class: safe_cex_engine::request_budget::RequestClass,
    ) -> HttpPrepareFuture {
        self.prepare_http(request, intent)
    }
    /// Open a socket; report it on `events` and return where to send text.
    /// Dropping the returned sender closes the socket.
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String>;
    /// Unsupported transports report a close instead of dropping authentication.
    fn connect_with_headers(
        &self,
        socket: SocketId,
        _url: String,
        _headers: Vec<(&'static str, String)>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsClosed(socket, 1006));
        mpsc::unbounded_channel().0
    }
    /// Explicit channel roles let sockets sharing an upstream URL route independently.
    fn connect_channel(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        _channel: Option<safe_cex_engine::SocketChannel>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        if headers.is_empty() {
            self.connect(socket, url, events)
        } else {
            self.connect_with_headers(socket, url, headers, events)
        }
    }
    /// Feed identity and host recovery hints stay outside venue routing policy.
    fn connect_context(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        _context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_channel(socket, url, headers, channel, events)
    }
    /// Binary protocols require explicit transport support.
    #[allow(clippy::too_many_arguments)]
    fn connect_encoding(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        if binary {
            let _ = events.send(HostEvent::WsClosed(socket, 1003));
            mpsc::unbounded_channel().0
        } else {
            self.connect_context(socket, url, headers, channel, context, events)
        }
    }
    /// Opt in only when this connection enforces WebSocketWriter's final claim.
    #[allow(clippy::too_many_arguments)]
    fn connect_socket(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> SocketConnection {
        SocketConnection {
            outgoing: self.connect_encoding(binary, socket, url, headers, channel, context, events),
            writer: None,
        }
    }
    /// Run a request, reporting its start, body chunks and end on `events`.
    /// Bound its lifetime and report `HttpFailed` on timeout, including the body.
    fn request(&self, request: HttpRequest, events: Events);
}

/// Real sockets (tungstenite over rustls) and HTTP (reqwest), body streamed.
#[derive(Clone)]
pub struct NetworkTransport {
    http: reqwest::Client,
    routing: Option<Arc<RoutingState>>,
    io_guard: Option<IoGuard>,
    venue: Option<Arc<str>>,
}

pub(crate) type IoGuard = Arc<dyn Fn() -> bool + Send + Sync>;
fn io_allowed(guard: &Option<IoGuard>) -> bool {
    guard.as_ref().is_none_or(|allowed| allowed())
}

impl NetworkTransport {
    pub fn new() -> Self {
        let _ = rustls::crypto::ring::default_provider().install_default();
        Self {
            http: reqwest::Client::builder()
                .timeout(Duration::from_secs(20))
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .expect("an HTTP client with the default configuration"),
            routing: None,
            io_guard: None,
            venue: None,
        }
    }

    /// Attributes this transport's failure reports to `venue`.
    pub fn with_venue(mut self, venue: &str) -> Self {
        self.venue = Some(venue.into());
        self
    }

    /// A host-owned, generation-bound authority checked at each final IO handoff.
    pub fn with_io_guard(mut self, guard: Arc<dyn Fn() -> bool + Send + Sync>) -> Self {
        self.io_guard = Some(guard);
        self
    }

    /// Apply the account plan resolved once by TypeScript, outside the engine.
    pub fn with_routing(plan: RoutingPlan, tokens: ProxyTokenProvider) -> Self {
        Self {
            routing: Some(Arc::new(RoutingState::new(plan, tokens))),
            ..Self::new()
        }
    }
}

impl Default for NetworkTransport {
    fn default() -> Self {
        Self::new()
    }
}

/// How long one resolved address may take to accept TCP before the next is tried.
/// The OS default (about two minutes on Linux) would stall failover behind a dead host.
const CONNECT_TIMEOUT: Duration = Duration::from_secs(4);

pub(crate) async fn open_socket(
    url: &str,
    headers: Vec<(String, String)>,
) -> Result<crate::routing::Socket, String> {
    let mut request = url
        .into_client_request()
        .map_err(|error| error.to_string())?;
    for (name, value) in headers {
        request.headers_mut().insert(
            HeaderName::from_bytes(name.as_bytes()).map_err(|error| error.to_string())?,
            HeaderValue::from_str(&value).map_err(|error| error.to_string())?,
        );
    }
    let (host, port) = socket_authority(&request)?;
    let addresses: Vec<std::net::SocketAddr> = tokio::net::lookup_host((host.as_str(), port))
        .await
        .map_err(|error| tokio_tungstenite::tungstenite::Error::Io(error).to_string())?
        .collect();
    open_socket_at(request, &addresses, CONNECT_TIMEOUT).await
}

/// The host and port `connect_async` would dial: the URL's, or the scheme's default.
fn socket_authority(request: &Request) -> Result<(String, u16), String> {
    let uri = request.uri();
    let host = uri
        .host()
        .map(|host| {
            host.trim_start_matches('[')
                .trim_end_matches(']')
                .to_owned()
        })
        .ok_or("URL error: No host name in the URL")?;
    let port = uri
        .port_u16()
        .or(match uri.scheme_str() {
            Some("wss") => Some(443),
            Some("ws") => Some(80),
            _ => None,
        })
        .ok_or("URL error: URL scheme not supported")?;
    Ok((host, port))
}

/// Connects to the first address that answers, then runs TLS and the upgrade over it;
/// SNI and `Host` still name the URL's host, so stock-root verification holds.
async fn open_socket_at(
    request: Request,
    addresses: &[std::net::SocketAddr],
    timeout: Duration,
) -> Result<crate::routing::Socket, String> {
    let (stream, index) = connect_first(addresses, timeout).await?;
    if index > 0 {
        eprintln!(
            "venue socket for {} connected to {} (address {} of {})",
            request.uri().host().unwrap_or_default(),
            addresses[index],
            index + 1,
            addresses.len()
        );
    }
    tokio_tungstenite::client_async_tls_with_config(request, stream, None, None)
        .await
        .map(|(socket, _)| socket)
        .map_err(|error| error.to_string())
}

/// Each address in turn, each bounded by `timeout`; the error is the last address's.
async fn connect_first(
    addresses: &[std::net::SocketAddr],
    timeout: Duration,
) -> Result<(tokio::net::TcpStream, usize), String> {
    let mut last = std::io::Error::new(std::io::ErrorKind::NotFound, "no address resolved");
    for (index, address) in addresses.iter().enumerate() {
        match tokio::time::timeout(timeout, tokio::net::TcpStream::connect(address)).await {
            Ok(Ok(stream)) => return Ok((stream, index)),
            Ok(Err(error)) => last = error,
            Err(_) => {
                last = std::io::Error::new(
                    std::io::ErrorKind::TimedOut,
                    format!("connect to {address} timed out after {timeout:?}"),
                )
            }
        }
    }
    Err(tokio_tungstenite::tungstenite::Error::Io(last).to_string())
}

/// How often one host's failed opens are logged; the line counts those in between.
const OPEN_FAILURE_LOG_EVERY: Duration = Duration::from_secs(60);

/// Failed venue socket opens, far-side closes and failed requests by host, so a refusal loop logs once a minute, not per retry.
#[derive(Default)]
struct OpenFailures {
    /// When each host last logged, and its failures since.
    by_host: std::collections::HashMap<String, (Option<std::time::Instant>, u32)>,
}

impl OpenFailures {
    /// The log line for a failed open of `url`, when one is due. The host only: a
    /// relay URL's query names its upstream and may carry a token.
    fn note(&mut self, url: &str, error: &str, now: std::time::Instant) -> Option<String> {
        self.line(
            format!("venue socket open failed: {}", host_of(url)),
            error,
            now,
        )
    }

    /// The log line for an open venue socket that closed without the engine asking.
    fn note_close(
        &mut self,
        url: &str,
        code: u16,
        reason: &str,
        open_for: Duration,
        now: std::time::Instant,
    ) -> Option<String> {
        let detail = format!("{code} {reason}, open {}s", open_for.as_secs());
        self.line(
            format!("venue socket closed: {}", host_of(url)),
            &detail,
            now,
        )
    }

    /// The log line for a venue request that failed in transport, by its route.
    fn note_request(
        &mut self,
        route: &str,
        error: &str,
        elapsed: Duration,
        now: std::time::Instant,
    ) -> Option<String> {
        let error = format!("{error} after {:.1}s", elapsed.as_secs_f64());
        self.line(format!("venue request failed: {route}"), &error, now)
    }

    fn line(&mut self, what: String, error: &str, now: std::time::Instant) -> Option<String> {
        let entry = self.by_host.entry(what.clone()).or_insert((None, 0));
        entry.1 += 1;
        if entry
            .0
            .is_some_and(|logged| now.duration_since(logged) < OPEN_FAILURE_LOG_EVERY)
        {
            return None;
        }
        let failures = std::mem::take(&mut entry.1);
        entry.0 = Some(now);
        Some(
            serde_json::json!({
                "level": "warn",
                "source": "safe-cex-native",
                "msg": format!("{what}: {error} ({failures} failures since the last line)"),
            })
            .to_string(),
        )
    }
}

static FAILURES: std::sync::Mutex<Option<OpenFailures>> = std::sync::Mutex::new(None);

/// Whether a line was due (and written).
fn log_failure(note: impl FnOnce(&mut OpenFailures) -> Option<String>) -> bool {
    let line = note(
        FAILURES
            .lock()
            .unwrap()
            .get_or_insert_with(OpenFailures::default),
    );
    if let Some(line) = &line {
        eprintln!("{line}");
    }
    line.is_some()
}

/// A transport failure the journal just logged, for the host's error sink.
fn report_failure(kind: crate::report::ErrorKind, target: &str, venue: Option<&str>) {
    crate::report::report(&crate::report::ErrorReport {
        kind,
        label: None,
        target: Some(target),
        error_kind: None,
        venue_code: None,
        venue,
        startup_unloaded: None,
        startup_refused: None,
    });
}

/// A URL's host alone: a relay URL's query names its upstream and may carry a token.
pub(crate) fn host_of(url: &str) -> String {
    url.parse::<tokio_tungstenite::tungstenite::http::Uri>()
        .ok()
        .and_then(|uri| uri.host().map(str::to_owned))
        .unwrap_or_else(|| "unparsable url".to_owned())
}

pub(crate) fn log_open_failure(url: &str, error: &str, venue: Option<&str>) {
    if log_failure(|failures| failures.note(url, error, std::time::Instant::now())) {
        report_failure(
            crate::report::ErrorKind::SocketOpenFailed,
            &host_of(url),
            venue,
        );
    }
}

/// A venue request that failed in transport (a timeout, a refused connection), by route.
pub(crate) fn log_request_failure(
    route: &str,
    error: &str,
    elapsed: Duration,
    venue: Option<&str>,
) {
    if log_failure(|failures| {
        failures.note_request(route, error, elapsed, std::time::Instant::now())
    }) {
        report_failure(crate::report::ErrorKind::RequestFailed, route, venue);
    }
}

const MAX_DECOMPRESSED_BYTES: usize = 16 * 1024 * 1024;

fn decode_message(bytes: &[u8]) -> std::io::Result<String> {
    let mut decoded = Vec::new();
    let mut reader: Box<dyn Read + '_> = if bytes.starts_with(&[0x1f, 0x8b]) {
        Box::new(flate2::read::GzDecoder::new(bytes))
    } else if bytes.first() == Some(&0x78) {
        Box::new(flate2::read::ZlibDecoder::new(bytes))
    } else {
        Box::new(bytes)
    };
    reader
        .by_ref()
        .take((MAX_DECOMPRESSED_BYTES + 1) as u64)
        .read_to_end(&mut decoded)?;
    if decoded.len() > MAX_DECOMPRESSED_BYTES {
        return Err(std::io::Error::other("WebSocket message exceeds limit"));
    }
    String::from_utf8(decoded).map_err(std::io::Error::other)
}

impl Transport for NetworkTransport {
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_with_headers(socket, url, vec![], events)
    }

    fn connect_with_headers(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_channel(socket, url, headers, None, events)
    }

    fn connect_channel(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_context(socket, url, headers, channel, None, events)
    }

    fn connect_context(
        &self,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_encoding(false, socket, url, headers, channel, context, events)
    }

    fn connect_encoding(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.connect_socket(binary, socket, url, headers, channel, context, events)
            .outgoing
    }

    fn connect_socket(
        &self,
        binary: bool,
        socket: SocketId,
        url: String,
        headers: Vec<(&'static str, String)>,
        channel: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
        events: Events,
    ) -> SocketConnection {
        let (outgoing, mut pending) = mpsc::unbounded_channel::<String>();
        let routing = self.routing.clone();
        let guard = self.io_guard.clone();
        let venue = self.venue.clone();
        let headers = headers
            .into_iter()
            .map(|(name, value)| (name.to_owned(), value))
            .collect();
        let (selected, direct_headers) = if let Some(state) = &routing {
            (
                Some(state.socket(&url, headers, channel, context)),
                Vec::new(),
            )
        } else {
            (None, headers)
        };
        let writer = websocket::NetworkWriter::new(socket, events.clone(), guard.clone());
        let driver_writer = writer.clone();
        tokio::spawn(async move {
            let writer = driver_writer;
            let _driver = writer.driver();
            let canceled = events.clone();
            let operation = async move {
                if !io_allowed(&guard) {
                    let _ = events.send(HostEvent::WsClosed(socket, 4100));
                    return;
                }
                // Enqueued as ordinary ingress after connect returned; no open or close follows.
                if selected.as_ref().is_some_and(|selected| selected.exhausted) {
                    let _ = events.send(HostEvent::WsRouteExhausted(socket));
                    return;
                }
                let (target, headers, relay, key) = match selected {
                    Some(selected) => (
                        selected.url,
                        selected.headers,
                        selected.relay,
                        Some(selected.key),
                    ),
                    None => (url, direct_headers, None, None),
                };
                let opening = async {
                    let mut ws = open_socket(&target, headers).await?;
                    if let Some(upstream) = relay {
                        let state = routing.as_ref().ok_or("relay requires routing")?;
                        authenticate_relay(&mut ws, &state.tokens, &upstream).await?;
                    }
                    Ok::<_, String>(ws)
                };
                let opening = tokio::time::timeout(Duration::from_secs(15), opening);
                tokio::pin!(opening);
                let mut queued = Vec::new();
                let mut ws = loop {
                    tokio::select! {
                        biased;
                        message = pending.recv() => match message {
                            Some(message) => queued.push(message),
                            None => return,
                        },
                        result = &mut opening => match result {
                            Ok(Ok(ws)) => break ws,
                            Ok(Err(error)) => { log_open_failure(&target, &error, venue.as_deref()); let _ = events.send(HostEvent::WsClosed(socket, 1006)); return; }
                            Err(_) => { log_open_failure(&target, "timed out after 15s", venue.as_deref()); let _ = events.send(HostEvent::WsClosed(socket, 1006)); return; }
                        },
                    }
                };
                if pending.is_closed() {
                    if !io_allowed(&guard) {
                        return;
                    }
                    let _ = tokio::time::timeout(Duration::from_secs(5), ws.close(None)).await;
                    return;
                }
                writer.opened(ws);
                for message in queued {
                    if !io_allowed(&guard) {
                        let _ = events.send(HostEvent::WsClosed(socket, 4100));
                        return;
                    }
                    if !matches!(
                        tokio::time::timeout(Duration::from_secs(5), writer.send(message)).await,
                        Ok(Ok(()))
                    ) {
                        let _ = events.send(HostEvent::WsClosed(socket, 1006));
                        return;
                    }
                }
                writer.enable_writes();
                let _ = events.send(HostEvent::WsOpened(socket));
                let opened_at = std::time::Instant::now();
                let (code, reason) = loop {
                    tokio::select! {
                        message = writer.next() => match message {
                            Ok(Some(Message::Text(text))) => {
                                if let (Some(state), Some(key)) = (&routing, &key) { state.message(key, &text); }
                                let _ = events.send(HostEvent::WsMessage(socket, text.to_string()));
                            }
                            Ok(Some(Message::Binary(bytes))) if binary => {
                                if let (Some(state), Some(key)) = (&routing, &key) { state.binary_message(key); }
                                let _ = events.send(HostEvent::WsBinary(socket, bytes.to_vec()));
                            }
                            Ok(Some(Message::Binary(bytes))) => {
                                let Ok(text) = decode_message(&bytes) else { break (1007, "undecodable binary frame".to_owned()) };
                                if let (Some(state), Some(key)) = (&routing, &key) { state.message(key, &text); }
                                let _ = events.send(HostEvent::WsMessage(socket, text));
                            }
                            Ok(Some(Message::Ping(_))) => {
                                if !io_allowed(&guard) { break (4100, "account IO authority expired".to_owned()); }
                                if !matches!(tokio::time::timeout(Duration::from_secs(5), writer.flush()).await, Ok(Ok(()))) { break (1006, "pong send failed".to_owned()); }
                            }
                            Ok(Some(Message::Close(frame))) => break frame.map_or((1005, String::new()), |frame| (u16::from(frame.code), frame.reason.to_string())),
                            Ok(Some(_)) => {}
                            Err(error) => break (1006, error.to_string()),
                            Ok(None) => break (1006, "stream ended".to_owned()),
                        },
                        text = pending.recv() => match text {
                            Some(text) => {
                                if !io_allowed(&guard) { break (4100, "account IO authority expired".to_owned()); }
                                if !matches!(tokio::time::timeout(Duration::from_secs(5), writer.send(text)).await, Ok(Ok(()))) { break (1006, "send failed".to_owned()); }
                            }
                            None => { if io_allowed(&guard) { let _ = tokio::time::timeout(Duration::from_secs(5), writer.close()).await; } return; }
                        },
                    }
                };
                let _ = log_failure(|failures| {
                    failures.note_close(
                        &target,
                        code,
                        &reason,
                        opened_at.elapsed(),
                        std::time::Instant::now(),
                    )
                });
                let _ = events.send(HostEvent::WsClosed(socket, code));
            };
            tokio::select! {
                biased;
                _ = canceled.closed() => {},
                _ = operation => {},
            }
        });
        SocketConnection {
            outgoing,
            writer: Some(Arc::new(writer)),
        }
    }

    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        http::prepare(self, request, Some(intent), None)
    }

    fn prepare_http_classified(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
        class: safe_cex_engine::request_budget::RequestClass,
    ) -> HttpPrepareFuture {
        http::prepare(self, request, Some(intent), Some(class))
    }

    fn request(&self, request: HttpRequest, events: Events) {
        // The transitional path uses the same prepared executor. Only its route
        // classification remains legacy until its venue declares context.
        let id = request.id;
        let packet = request.clone();
        let preparation = http::prepare(self, request, None, None);
        tokio::spawn(async move {
            let result = tokio::select! {
                biased;
                _ = events.closed() => return,
                result = preparation => result,
            };
            match result.and_then(|prepared| prepared.bind(packet)) {
                Ok(bound) => {
                    bound.dispatch(events);
                }
                Err(error) => {
                    let _ = events.send(HostEvent::HttpFailed(id, error.message().into()));
                }
            }
        });
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use futures_util::{SinkExt, StreamExt};
    use std::io::Write;

    #[tokio::test]
    async fn io_guard_drops_a_queued_socket_write_when_the_open_finishes_after_fencing() {
        use std::sync::atomic::{AtomicBool, Ordering};
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("ws://{}/private", listener.local_addr().unwrap());
        let (connected, waiting) = tokio::sync::oneshot::channel();
        let (release, held) = tokio::sync::oneshot::channel();
        let server = tokio::spawn(async move {
            let (tcp, _) = listener.accept().await.unwrap();
            connected.send(()).unwrap();
            held.await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(tcp).await.unwrap();
            let message = tokio::time::timeout(Duration::from_secs(2), socket.next())
                .await
                .unwrap();
            assert!(!matches!(message, Some(Ok(Message::Text(_)))));
        });
        let allowed = Arc::new(AtomicBool::new(true));
        let gate = allowed.clone();
        let transport =
            NetworkTransport::new().with_io_guard(Arc::new(move || gate.load(Ordering::Acquire)));
        let (events, mut replies) = mpsc::unbounded_channel();
        let socket = transport.connect(SocketId(1), url, events);
        socket.send("queued-order-sentinel".into()).unwrap();
        waiting.await.unwrap();
        allowed.store(false, Ordering::Release);
        release.send(()).unwrap();
        tokio::time::timeout(Duration::from_secs(2), async {
            loop {
                if matches!(
                    replies.recv().await,
                    Some(HostEvent::WsClosed(SocketId(1), 4100))
                ) {
                    break;
                }
            }
        })
        .await
        .unwrap();
        server.await.unwrap();
    }

    #[test]
    fn a_failed_venue_socket_open_is_logged_by_host_at_most_once_a_minute() {
        let mut failures = OpenFailures::default();
        let start = std::time::Instant::now();
        let url = "wss://relay.example/tunnel?ts-relay=wss%3A%2F%2Fvenue&token=abc";
        let line = failures
            .note(url, "HTTP error: 429 Too Many Requests", start)
            .unwrap();
        assert!(
            line.contains("relay.example") && line.contains("429"),
            "{line}"
        );
        assert!(
            !line.contains("token") && !line.contains("ts-relay"),
            "never the URI: {line}"
        );
        for second in 1..60 {
            let at = start + Duration::from_secs(second);
            assert!(failures.note(url, "timed out after 15s", at).is_none());
        }
        let other = failures.note(
            "wss://other.example/ws",
            "IO error",
            start + Duration::from_secs(1),
        );
        assert!(other.is_some(), "each host on its own");
        let again = failures
            .note(url, "timed out after 15s", start + Duration::from_secs(60))
            .unwrap();
        assert!(again.contains("60 failures"), "{again}");
    }

    #[test]
    fn a_failed_venue_request_is_logged_by_route_with_its_elapsed_time() {
        let mut failures = OpenFailures::default();
        let start = std::time::Instant::now();
        let line = failures
            .note_request(
                "api.bitget.com via proxy-jp.example",
                "transport request timed out",
                Duration::from_millis(20_004),
                start,
            )
            .unwrap();
        assert!(
            line.contains("venue request failed: api.bitget.com via proxy-jp.example")
                && line.contains("timed out")
                && line.contains("after 20.0s"),
            "{line}"
        );
        assert!(
            failures
                .note_request(
                    "api.bitget.com via proxy-jp.example",
                    "x",
                    Duration::ZERO,
                    start
                )
                .is_none(),
            "throttled like socket opens"
        );
    }

    #[test]
    fn a_venue_socket_closed_from_the_far_side_is_logged_with_code_reason_and_age() {
        let mut failures = OpenFailures::default();
        let start = std::time::Instant::now();
        let url = "wss://relay.example/tunnel?ts-relay=wss%3A%2F%2Fvenue&token=abc";
        let line = failures
            .note_close(url, 1000, "idle timeout", Duration::from_secs(31), start)
            .unwrap();
        assert!(
            line.contains("venue socket closed: relay.example")
                && line.contains("1000 idle timeout")
                && line.contains("open 31s"),
            "{line}"
        );
        assert!(
            !line.contains("token") && !line.contains("ts-relay"),
            "{line}"
        );
        assert!(
            failures
                .note_close(url, 1000, "idle timeout", Duration::from_secs(30), start)
                .is_none(),
            "throttled per host"
        );
    }

    /// A refused upgrade names its status: Cloudflare's 429 or 403 is the evidence.
    #[tokio::test]
    async fn a_refused_websocket_upgrade_reports_its_http_status() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let port = listener.local_addr().unwrap().port();
        tokio::spawn(async move {
            use tokio::io::{AsyncReadExt, AsyncWriteExt};
            let (mut stream, _) = listener.accept().await.unwrap();
            let mut request = [0u8; 2048];
            let _ = stream.read(&mut request).await;
            let _ = stream
                .write_all(b"HTTP/1.1 429 Too Many Requests\r\ncontent-length: 0\r\n\r\n")
                .await;
        });
        let error = open_socket(&format!("ws://127.0.0.1:{port}/tunnel"), vec![])
            .await
            .err()
            .unwrap();
        assert!(error.contains("429"), "{error}");
    }

    /// A WebSocket listener on loopback that completes one upgrade.
    async fn upgrade_once() -> std::net::SocketAddr {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            let (stream, _) = listener.accept().await.unwrap();
            let mut socket = tokio_tungstenite::accept_async(stream).await.unwrap();
            let _ = socket.next().await;
        });
        address
    }

    /// A non-routable address: its SYN goes unanswered, so only the timeout ends the try.
    const BLACKHOLE: &str = "10.255.255.1:443";

    #[tokio::test]
    async fn a_silent_first_address_fails_over_to_the_second_within_the_timeout() {
        let live = upgrade_once().await;
        let request = format!("ws://tunnel-vpc.tealstreet.io:{}/tunnel", live.port())
            .into_client_request()
            .unwrap();
        let started = std::time::Instant::now();
        let mut socket = open_socket_at(
            request,
            &[BLACKHOLE.parse().unwrap(), live],
            Duration::from_millis(300),
        )
        .await
        .unwrap();
        assert!(
            started.elapsed() < Duration::from_secs(2),
            "{:?}",
            started.elapsed()
        );
        tokio::time::timeout(Duration::from_secs(1), socket.send(Message::text("ping")))
            .await
            .unwrap()
            .unwrap();
    }

    #[tokio::test]
    async fn the_first_answering_address_wins_and_a_silent_one_reports_its_timeout() {
        let live = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let live = live.local_addr().map(|a| (live, a)).unwrap();
        let (_, index) = connect_first(
            &[live.1, BLACKHOLE.parse().unwrap()],
            Duration::from_millis(300),
        )
        .await
        .unwrap();
        assert_eq!(index, 0);
        let error = connect_first(&[BLACKHOLE.parse().unwrap()], Duration::from_millis(200))
            .await
            .err()
            .unwrap();
        assert!(
            error.starts_with("IO error: ")
                && (error.contains("timed out") || error.contains("unreachable")),
            "{error}"
        );
        assert_eq!(
            connect_first(&[], Duration::from_millis(1))
                .await
                .err()
                .unwrap(),
            "IO error: no address resolved"
        );
    }

    #[test]
    fn the_dialled_authority_defaults_the_port_by_scheme() {
        let authority = |url: &str| socket_authority(&url.into_client_request().unwrap());
        assert_eq!(
            authority("wss://tunnel-vpc.tealstreet.io/tunnel").unwrap(),
            ("tunnel-vpc.tealstreet.io".into(), 443)
        );
        assert_eq!(authority("ws://[::1]:9/x").unwrap(), ("::1".into(), 9));
    }

    #[test]
    fn compressed_messages_cannot_expand_past_limit() {
        for gzip in [false, true] {
            let data = vec![b'x'; MAX_DECOMPRESSED_BYTES + 1];
            let compressed = if gzip {
                let mut encoder =
                    flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap()
            } else {
                let mut encoder =
                    flate2::write::ZlibEncoder::new(Vec::new(), flate2::Compression::fast());
                encoder.write_all(&data).unwrap();
                encoder.finish().unwrap()
            };
            assert!(decode_message(&compressed).is_err());
        }
        assert_eq!(decode_message(b"valid text").unwrap(), "valid text");
    }

    type Reported = (&'static str, String, Option<String>);
    static REPORTED: std::sync::Mutex<Vec<Reported>> = std::sync::Mutex::new(Vec::new());

    fn record(report: &crate::report::ErrorReport<'_>) {
        REPORTED.lock().unwrap().push((
            report.kind.as_str(),
            report.target.unwrap_or_default().to_owned(),
            report.venue.map(str::to_owned),
        ));
    }

    /// The sink is process-wide, so one test owns it: direct logging, then a
    /// real refused request through a venue-attributed transport. Its route
    /// (`localhost`) is one no other test in this binary logs inside the window.
    #[tokio::test]
    async fn a_logged_failure_reaches_the_error_sink_once_per_window_with_its_venue() {
        assert!(crate::report::set_error_sink(record));
        let url = "wss://sink-test.example/tunnel?ts-relay=wss%3A%2F%2Fvenue&token=t";
        log_open_failure(url, "refused", Some("bybitdemo"));
        log_open_failure(url, "refused", Some("bybitdemo"));
        log_request_failure(
            "sink-test.example via HTTP tunnel",
            "reset",
            Duration::ZERO,
            None,
        );
        let closed = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
        let address = closed.local_addr().unwrap();
        drop(closed);
        let (events, mut received) = mpsc::unbounded_channel();
        NetworkTransport::new().with_venue("okx").request(
            HttpRequest {
                id: safe_cex_engine::RequestId(1),
                method: safe_cex_engine::Method::Get,
                route: None,
                timeout_ms: Some(5_000),
                url: format!("http://localhost:{}/v5/order?sign=s", address.port()),
                headers: vec![],
                signer_headers: vec![],
                body: None,
            },
            events,
        );
        assert!(matches!(
            received.recv().await,
            Some(HostEvent::HttpFailed(..))
        ));
        let reported: Vec<_> = REPORTED
            .lock()
            .unwrap()
            .iter()
            .filter(|(_, target, _)| {
                target.starts_with("sink-test.example") || target.starts_with("localhost")
            })
            .cloned()
            .collect();
        assert_eq!(
            reported,
            [
                (
                    "transport.socket_open_failed",
                    "sink-test.example".to_owned(),
                    Some("bybitdemo".to_owned())
                ),
                (
                    "transport.request_failed",
                    "sink-test.example via HTTP tunnel".to_owned(),
                    None
                ),
                (
                    "transport.request_failed",
                    "localhost direct".to_owned(),
                    Some("okx".to_owned())
                ),
            ]
        );
    }
}
