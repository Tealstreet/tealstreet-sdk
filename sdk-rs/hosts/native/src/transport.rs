//! Where a host's sockets and requests go: the network, or a recording.
use crate::routing::{ProxyTokenProvider, RoutingPlan, RoutingState, authenticate_relay};
use crate::{Events, HostEvent};
use futures_util::{SinkExt, StreamExt};
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
    http::{HeaderName, HeaderValue},
};

mod http;
pub use http::{BoundHttp, HttpPrepareError, HttpPrepareFuture, PreparedHttp};

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
    /// Run a request, reporting its start, body chunks and end on `events`.
    /// Bound its lifetime and report `HttpFailed` on timeout, including the body.
    fn request(&self, request: HttpRequest, events: Events);
}

/// Real sockets (tungstenite over rustls) and HTTP (reqwest), body streamed.
#[derive(Clone)]
pub struct NetworkTransport {
    http: reqwest::Client,
    routing: Option<Arc<RoutingState>>,
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
        }
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

async fn open_socket(
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
    tokio_tungstenite::connect_async(request)
        .await
        .map(|(socket, _)| socket)
        .map_err(|error| error.to_string())
}

/// How often one host's failed opens are logged; the line counts those in between.
const OPEN_FAILURE_LOG_EVERY: Duration = Duration::from_secs(60);

/// Failed venue socket opens and requests by host, so a refusal loop logs once a minute, not per retry.
#[derive(Default)]
struct OpenFailures {
    /// When each host last logged, and its failures since.
    by_host: std::collections::HashMap<String, (Option<std::time::Instant>, u32)>,
}

impl OpenFailures {
    /// The log line for a failed open of `url`, when one is due. The host only: a
    /// relay URL's query names its upstream and may carry a token.
    fn note(&mut self, url: &str, error: &str, now: std::time::Instant) -> Option<String> {
        let host = url
            .parse::<tokio_tungstenite::tungstenite::http::Uri>()
            .ok()
            .and_then(|uri| uri.host().map(str::to_owned))
            .unwrap_or_else(|| "unparsable url".to_owned());
        self.line(format!("venue socket open failed: {host}"), error, now)
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

fn log_failure(note: impl FnOnce(&mut OpenFailures) -> Option<String>) {
    if let Some(line) = note(
        FAILURES
            .lock()
            .unwrap()
            .get_or_insert_with(OpenFailures::default),
    ) {
        eprintln!("{line}");
    }
}

fn log_open_failure(url: &str, error: &str) {
    log_failure(|failures| failures.note(url, error, std::time::Instant::now()));
}

/// A venue request that failed in transport (a timeout, a refused connection), by route.
pub(crate) fn log_request_failure(route: &str, error: &str, elapsed: Duration) {
    log_failure(|failures| failures.note_request(route, error, elapsed, std::time::Instant::now()));
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
        let (outgoing, mut pending) = mpsc::unbounded_channel::<String>();
        let routing = self.routing.clone();
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
        tokio::spawn(async move {
            let canceled = events.clone();
            let operation = async move {
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
                            Ok(Err(error)) => { log_open_failure(&target, &error); let _ = events.send(HostEvent::WsClosed(socket, 1006)); return; }
                            Err(_) => { log_open_failure(&target, "timed out after 15s"); let _ = events.send(HostEvent::WsClosed(socket, 1006)); return; }
                        },
                    }
                };
                if pending.is_closed() {
                    let _ = tokio::time::timeout(Duration::from_secs(5), ws.close(None)).await;
                    return;
                }
                let _ = events.send(HostEvent::WsOpened(socket));
                for message in queued {
                    if !matches!(
                        tokio::time::timeout(
                            Duration::from_secs(5),
                            ws.send(Message::text(message))
                        )
                        .await,
                        Ok(Ok(()))
                    ) {
                        let _ = events.send(HostEvent::WsClosed(socket, 1006));
                        return;
                    }
                }
                let code = loop {
                    tokio::select! {
                        message = ws.next() => match message {
                            Some(Ok(Message::Text(text))) => {
                                if let (Some(state), Some(key)) = (&routing, &key) { state.message(key, &text); }
                                let _ = events.send(HostEvent::WsMessage(socket, text.to_string()));
                            }
                            Some(Ok(Message::Binary(bytes))) if binary => {
                                if let (Some(state), Some(key)) = (&routing, &key) { state.binary_message(key); }
                                let _ = events.send(HostEvent::WsBinary(socket, bytes.to_vec()));
                            }
                            Some(Ok(Message::Binary(bytes))) => {
                                let Ok(text) = decode_message(&bytes) else { break 1007 };
                                if let (Some(state), Some(key)) = (&routing, &key) { state.message(key, &text); }
                                let _ = events.send(HostEvent::WsMessage(socket, text));
                            }
                            Some(Ok(Message::Ping(_))) => { if !matches!(tokio::time::timeout(Duration::from_secs(5), ws.flush()).await, Ok(Ok(()))) { break 1006; } }
                            Some(Ok(Message::Close(frame))) => break frame.map_or(1005, |frame| u16::from(frame.code)),
                            Some(Ok(_)) => {}
                            Some(Err(_)) | None => break 1006,
                        },
                        text = pending.recv() => match text {
                            Some(text) => { if !matches!(tokio::time::timeout(Duration::from_secs(5), ws.send(Message::text(text))).await, Ok(Ok(()))) { break 1006; } }
                            None => { let _ = tokio::time::timeout(Duration::from_secs(5), ws.close(None)).await; return; }
                        },
                    }
                };
                let _ = events.send(HostEvent::WsClosed(socket, code));
            };
            tokio::select! {
                biased;
                _ = canceled.closed() => {},
                _ = operation => {},
            }
        });
        outgoing
    }

    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        http::prepare(self, request, Some(intent))
    }

    fn request(&self, request: HttpRequest, events: Events) {
        // The transitional path uses the same prepared executor. Only its route
        // classification remains legacy until its venue declares context.
        let id = request.id;
        let packet = request.clone();
        let preparation = http::prepare(self, request, None);
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
    use std::io::Write;

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
}
