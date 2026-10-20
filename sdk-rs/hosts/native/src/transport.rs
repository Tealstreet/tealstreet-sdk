//! Where a host's sockets and requests go: the network, or a recording.
use crate::routing::{
    HttpRouteMode, ProxyTokenProvider, RoutingPlan, RoutingState, authenticate_how,
    authenticate_relay, insert_header, request_how,
};
use crate::{Events, HostEvent};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, Method, SocketId};
use std::io::Read;
use std::sync::Arc;
use std::time::Duration;
use tokio::sync::mpsc;
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    http::{HeaderName, HeaderValue},
};

const MAX_BINARY_BYTES: usize = 16 * 1024 * 1024;

fn binary_text(bytes: &[u8]) -> Result<String, u16> {
    let mut decoded = Vec::new();
    let limit = MAX_BINARY_BYTES as u64 + 1;
    if bytes.starts_with(&[0x1f, 0x8b]) {
        flate2::read::GzDecoder::new(bytes)
            .take(limit)
            .read_to_end(&mut decoded)
            .map_err(|_| 1007u16)?;
    } else if bytes.first() == Some(&0x78)
        && bytes
            .get(1)
            .is_some_and(|b| (0x7800 + u16::from(*b)).is_multiple_of(31))
    {
        flate2::read::ZlibDecoder::new(bytes)
            .take(limit)
            .read_to_end(&mut decoded)
            .map_err(|_| 1007u16)?;
    } else {
        if bytes.len() > MAX_BINARY_BYTES {
            return Err(1009);
        }
        decoded.extend_from_slice(bytes);
    }
    if decoded.len() > MAX_BINARY_BYTES {
        return Err(1009);
    }
    String::from_utf8(decoded).map_err(|_| 1007)
}

pub trait Transport {
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
                let _ = ws.close(None).await;
                return;
            }
            let _ = events.send(HostEvent::WsOpened(socket));
            for message in queued {
                if ws.send(Message::text(message)).await.is_err() {
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
                        Some(Ok(Message::Binary(bytes))) => {
                            let text = match binary_text(&bytes) {
                                Ok(text) => text,
                                Err(code) => break code,
                            };
                            if let (Some(state), Some(key)) = (&routing, &key) { state.message(key, &text); }
                            let _ = events.send(HostEvent::WsMessage(socket, text));
                        }
                        Some(Ok(Message::Ping(_))) => { let _ = ws.flush().await; }
                        Some(Ok(Message::Close(frame))) => break frame.map_or(1005, |frame| u16::from(frame.code)),
                        Some(Ok(_)) => {}
                        Some(Err(_)) | None => break 1006,
                    },
                    text = pending.recv() => match text {
                        Some(text) => { if ws.send(Message::text(text)).await.is_err() { break 1006; } }
                        None => { let _ = ws.close(None).await; return; }
                    },
                }
            };
            let _ = events.send(HostEvent::WsClosed(socket, code));
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
                    .collect();
                if let Some(route) = &route {
                    if !route.prefix.is_empty() {
                        request.url = format!("{}/{}", route.prefix, request.url);
                    }
                    for (name, value) in &route.headers {
                        insert_header(&mut headers, name.clone(), value.clone());
                    }
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
                let response = builder.send().await.map_err(|error| error.to_string())?;
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
                    let chunk = chunk.map_err(|error| error.to_string())?;
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
mod tests {
    use super::*;
    use flate2::{
        Compression,
        write::{GzEncoder, ZlibEncoder},
    };
    use std::io::Write;

    fn gzip(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = GzEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    fn zlib(bytes: &[u8]) -> Vec<u8> {
        let mut encoder = ZlibEncoder::new(Vec::new(), Compression::fast());
        encoder.write_all(bytes).unwrap();
        encoder.finish().unwrap()
    }

    #[test]
    fn binary_frames_decode_plain_gzip_and_zlib_utf8() {
        let text = "{\"book\":\"€\"}";
        for bytes in [
            text.as_bytes().to_vec(),
            gzip(text.as_bytes()),
            zlib(text.as_bytes()),
        ] {
            assert_eq!(binary_text(&bytes).unwrap(), text);
        }
    }

    #[test]
    fn corrupt_compressed_frames_and_invalid_utf8_are_rejected_whole() {
        let mut corrupted = gzip(b"{\"book\":1}");
        let crc = corrupted.len() - 8;
        corrupted[crc] ^= 1;
        for bytes in [corrupted, vec![0x1f, 0x8b, 0], vec![0xff], zlib(&[0xff])] {
            assert_eq!(binary_text(&bytes), Err(1007));
        }
    }

    #[test]
    fn decoded_binary_limit_rejects_oversize_without_returning_a_prefix() {
        let bytes = vec![b'a'; MAX_BINARY_BYTES + 1];
        for encoded in [gzip(&bytes), zlib(&bytes)] {
            assert_eq!(binary_text(&encoded), Err(1009));
        }
        assert_eq!(binary_text(&bytes), Err(1009));
    }
}
