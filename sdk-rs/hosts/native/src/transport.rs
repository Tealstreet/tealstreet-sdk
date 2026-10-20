//! Where a host's sockets and requests go: the network, or a recording.
use crate::routing::{
    HttpRouteMode, ProxyTokenProvider, RoutingPlan, RoutingState, authenticate_how,
    authenticate_relay, insert_header, request_how,
};
use crate::{Events, HostEvent};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, Method, PayloadSignRequest, RequestId, SocketId};
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
};

pub struct PayloadSignerResult {
    pub signature: String,
    pub headers: Vec<(String, String)>,
}
impl std::fmt::Debug for PayloadSignerResult {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PayloadSignerResult")
            .field("signature", &"<redacted>")
            .field("headers", &"<redacted>")
            .finish()
    }
}
pub type PayloadSignerFuture =
    Pin<Box<dyn Future<Output = Result<PayloadSignerResult, String>> + Send>>;
pub type PayloadSigner = Arc<dyn Fn(PayloadSignRequest) -> PayloadSignerFuture + Send + Sync>;

pub trait Transport {
    fn sign_payload(
        &self,
        request: RequestId,
        _payload: PayloadSignRequest,
        _timeout_ms: u32,
        events: Events,
    ) {
        let _ = events.send(HostEvent::PayloadSignFailed(
            request,
            "Payload signing is not configured".into(),
        ));
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
    signer: Option<PayloadSigner>,
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
            signer: None,
        }
    }

    /// Apply the account plan resolved once by TypeScript, outside the engine.
    pub fn with_routing(plan: RoutingPlan, tokens: ProxyTokenProvider) -> Self {
        Self {
            routing: Some(Arc::new(RoutingState::new(plan, tokens))),
            ..Self::new()
        }
    }
    pub fn with_payload_signer(mut self, signer: PayloadSigner) -> Self {
        self.signer = Some(signer);
        self
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
    fn sign_payload(
        &self,
        request: RequestId,
        payload: PayloadSignRequest,
        timeout_ms: u32,
        events: Events,
    ) {
        let signer = self.signer.clone();
        tokio::spawn(async move {
            let operation = async {
                let signer =
                    signer.ok_or_else(|| "Payload signing is not configured".to_owned())?;
                signer(payload)
                    .await
                    .map_err(|_| "Payload signing failed".to_owned())
            };
            let result = tokio::select! {
                biased;
                _ = events.closed() => return,
                result = tokio::time::timeout(Duration::from_millis(u64::from(timeout_ms.max(1))), operation) => result,
            };
            let event = match result {
                Ok(Ok(result)) => HostEvent::PayloadSigned(request, result),
                Ok(Err(error)) => HostEvent::PayloadSignFailed(request, error),
                Err(_) => HostEvent::PayloadSignFailed(request, "Payload signing timed out".into()),
            };
            let _ = events.send(event);
        });
    }
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
                            _ => { let _ = events.send(HostEvent::WsClosed(socket, 1006)); return; }
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

    fn request(&self, mut request: HttpRequest, events: Events) {
        let http = self.http.clone();
        let routing = self.routing.clone();
        tokio::spawn(async move {
            let id = request.id;
            let timeout =
                Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(20_000).max(1)));
            let operation = async {
                let route = match routing.as_ref() {
                    Some(state) => Some(state.http(&request)?),
                    None if request.route == Some(safe_cex_engine::RequestRoute::PublicCors) => {
                        return Err("Public CORS request routing is not configured".into());
                    }
                    None => None,
                };
                let mut headers: Vec<_> = request
                    .headers
                    .iter()
                    .map(|(name, value)| ((*name).to_owned(), value.clone()))
                    .chain(request.signer_headers.iter().cloned())
                    .collect();
                if let Some(route) = &route {
                    if !route.prefix.is_empty() {
                        request.url = format!("{}/{}", route.prefix, request.url);
                    }
                    for (name, value) in &route.headers {
                        insert_header(&mut headers, name.clone(), value.clone());
                    }
                    route.alias_headers(&mut headers)?;
                    if route.authorize {
                        let token = (routing.as_ref().unwrap().tokens)().await?;
                        if !token.is_empty() {
                            insert_header(
                                &mut headers,
                                route.authorization_header.clone(),
                                format!("Bearer {token}"),
                            );
                        }
                    }
                    if route.mode == HttpRouteMode::How {
                        let tunnel = route
                            .tunnel_url
                            .as_deref()
                            .ok_or("HOW route missing tunnelUrl")?;
                        let state = routing.as_ref().unwrap();
                        let upgrade = state
                            .plan
                            .ws_headers
                            .iter()
                            .map(|(name, value)| (name.clone(), value.clone()))
                            .collect();
                        let mut tunnel =
                            reqwest::Url::parse(tunnel).map_err(|error| error.to_string())?;
                        let query: Vec<_> = tunnel
                            .query_pairs()
                            .filter(|(name, _)| name != "v" && name != "t")
                            .map(|(name, value)| (name.into_owned(), value.into_owned()))
                            .collect();
                        tunnel.set_query(None);
                        tunnel
                            .query_pairs_mut()
                            .extend_pairs(query)
                            .append_pair("v", "2")
                            .append_pair("t", &format!("{:.0}", crate::now_ms()));
                        let mut ws = open_socket(tunnel.as_str(), upgrade).await?;
                        authenticate_how(&mut ws, &state.tokens).await?;
                        let result = request_how(&mut ws, &request, &headers, &events).await;
                        let _ = ws.close(None).await;
                        return result;
                    }
                }
                let method = match request.method {
                    Method::Get => reqwest::Method::GET,
                    Method::Post => reqwest::Method::POST,
                    Method::Put => reqwest::Method::PUT,
                    Method::Patch => reqwest::Method::PATCH,
                    Method::Delete => reqwest::Method::DELETE,
                };
                let mut builder = http.request(method, &request.url).timeout(timeout);
                for (name, value) in headers {
                    builder = builder.header(name, value);
                }
                if let Some(body) = request.body {
                    builder = builder.body(body);
                }
                let response = builder
                    .send()
                    .await
                    .map_err(|error| error.without_url().to_string())?;
                let headers = response
                    .headers()
                    .iter()
                    .filter_map(|(name, value)| Some(format!("{name}: {}", value.to_str().ok()?)))
                    .collect::<Vec<_>>()
                    .join("\n");
                let _ = events.send(HostEvent::HttpStart {
                    request: id,
                    status: response.status().as_u16(),
                    headers,
                });
                let mut body = response.bytes_stream();
                while let Some(chunk) = body.next().await {
                    let chunk = chunk.map_err(|error| error.without_url().to_string())?;
                    for piece in chunk.chunks(16 * 1024) {
                        let _ = events.send(HostEvent::HttpChunk(id, piece.to_vec()));
                    }
                }
                let _ = events.send(HostEvent::HttpEnd(id));
                Ok::<_, String>(())
            };
            let result = tokio::select! {
                biased;
                _ = events.closed() => return,
                result = tokio::time::timeout(timeout, operation) => result,
            };
            let failure = match result {
                Ok(Ok(())) => None,
                Ok(Err(error)) => Some(error),
                Err(_) => Some("transport request timed out".into()),
            };
            if let Some(error) = failure {
                let _ = events.send(HostEvent::HttpFailed(id, error));
            }
        });
    }
}

#[cfg(test)]
mod security_tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn delegated_signer_result_debug_redacts_signatures_and_headers() {
        let result = PayloadSignerResult {
            signature: "private-signature-sentinel".into(),
            headers: vec![("Authorization".into(), "private-token-sentinel".into())],
        };
        let debug = format!("{result:?}");
        assert!(!debug.contains("private-signature-sentinel"));
        assert!(!debug.contains("private-token-sentinel"));
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
