//! Preparation and final handoff for the existing native HTTP executor.
use super::{NetworkTransport, open_socket};
use crate::routing::{
    HttpRoute, HttpRouteMode, Socket, authenticate_how, http_packet, insert_header, receive_http,
};
use crate::{Events, HostEvent};
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::request_context::RoutingIntent;
use safe_cex_engine::{HttpRequest, Method, RequestId, RequestRoute};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::task::AbortHandle;
use tokio::time::Instant;
use tokio_tungstenite::tungstenite::Message;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpPrepareError {
    Unsupported,
    Expired,
    Invalid(&'static str),
    Transport(String),
}
impl HttpPrepareError {
    pub fn message(&self) -> &str {
        match self {
            Self::Unsupported => "HTTP transport preparation is unsupported",
            Self::Expired => "transport request timed out",
            Self::Invalid(message) => message,
            Self::Transport(message) => message,
        }
    }
}

pub type HttpPrepareFuture =
    Pin<Box<dyn Future<Output = Result<Box<dyn PreparedHttp>, HttpPrepareError>> + Send>>;

/// Route/token/tunnel preparation is complete. Bind the exact signed packet
/// before claiming release; this step can validate and allocate but cannot send.
pub trait PreparedHttp: Send + fmt::Debug {
    fn bind(self: Box<Self>, packet: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError>;
}

/// Held by the engine-owning actor. After its synchronous final claim, call
/// dispatch immediately: no message to a task which might resume much later.
pub trait BoundHttp: Send + fmt::Debug {
    fn ready(&self) -> bool;
    fn dispatch(self: Box<Self>, events: Events) -> AbortHandle;
}

struct Prepared {
    http: reqwest::Client,
    request: RequestId,
    method: Method,
    route_hint: Option<RequestRoute>,
    managed: bool,
    origin: String,
    deadline: Instant,
    route: Option<HttpRoute>,
    proxy_header: Option<(String, String)>,
    socket: Option<Socket>,
}
impl fmt::Debug for Prepared {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("PreparedHttp")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

enum Packet {
    Direct(reqwest::Client, Box<reqwest::Request>),
    How(Box<Socket>, String),
}
struct Bound {
    request: RequestId,
    deadline: Instant,
    packet: Packet,
    /// Sent through the Tealstreet REST proxy, whose own 401 is not the venue's.
    proxied: bool,
}
impl fmt::Debug for Bound {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BoundHttp")
            .field("request", &self.request)
            .finish_non_exhaustive()
    }
}

fn origin(url: &str) -> Result<String, HttpPrepareError> {
    let mut url =
        reqwest::Url::parse(url).map_err(|_| HttpPrepareError::Invalid("Invalid HTTP URL"))?;
    if !matches!(url.scheme(), "http" | "https")
        || !url.username().is_empty()
        || url.password().is_some()
    {
        return Err(HttpPrepareError::Invalid("Invalid HTTP URL"));
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url.into())
}

pub(super) fn prepare(
    transport: &NetworkTransport,
    request: HttpRequest,
    intent: Option<RoutingIntent>,
) -> HttpPrepareFuture {
    let http = transport.http.clone();
    let routing = transport.routing.clone();
    let deadline = Instant::now()
        + Duration::from_millis(u64::from(request.timeout_ms.unwrap_or(20_000).max(1)));
    Box::pin(async move {
        let operation = async {
            let origin = origin(&request.url)?;
            let route = match routing.as_ref() {
                Some(state) => Some(
                    match intent {
                        Some(intent) => state.managed_http(&request, intent),
                        None => state.http(&request),
                    }
                    .map_err(|_| HttpPrepareError::Invalid("HTTP route is unavailable"))?,
                ),
                None if request.route == Some(RequestRoute::PublicCors) => {
                    return Err(HttpPrepareError::Invalid(
                        "Public CORS request routing is not configured",
                    ));
                }
                None => None,
            };
            let mut proxy_header = None;
            let mut socket = None;
            if let Some(route) = &route {
                if route.authorize {
                    let state = routing.as_ref().expect("route has a routing state");
                    let token = (state.tokens)().await.map_err(|_| {
                        HttpPrepareError::Transport("Proxy authorization failed".into())
                    })?;
                    // Without a token the edge answers a plain 401 that venues read as a
                    // refused key; the request is refused here instead, as our own failure.
                    if token.is_empty() {
                        return Err(HttpPrepareError::Transport(
                            PROXY_AUTHORIZATION_MISSING.into(),
                        ));
                    }
                    proxy_header = Some((
                        route.authorization_header.clone(),
                        format!("Bearer {token}"),
                    ));
                }
                if route.mode == HttpRouteMode::How {
                    let state = routing.as_ref().expect("route has a routing state");
                    let tunnel = route
                        .tunnel_url
                        .as_deref()
                        .ok_or(HttpPrepareError::Invalid("HOW route missing tunnelUrl"))?;
                    let mut tunnel = reqwest::Url::parse(tunnel)
                        .map_err(|_| HttpPrepareError::Invalid("Invalid HTTP tunnel URL"))?;
                    let retained: Vec<_> = tunnel
                        .query_pairs()
                        .filter(|(name, _)| name != "v" && name != "t")
                        .map(|(name, value)| (name.into_owned(), value.into_owned()))
                        .collect();
                    tunnel.set_query(None);
                    tunnel
                        .query_pairs_mut()
                        .extend_pairs(retained)
                        .append_pair("v", "2")
                        .append_pair("t", &format!("{:.0}", crate::now_ms()));
                    let headers = state
                        .plan
                        .ws_headers
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect();
                    let mut connected =
                        open_socket(tunnel.as_str(), headers).await.map_err(|_| {
                            HttpPrepareError::Transport("HTTP tunnel connection failed".into())
                        })?;
                    // The helper already redacts provider failures. Preserve
                    // that caller-visible cause, without parsing error text.
                    authenticate_how(&mut connected, &state.tokens)
                        .await
                        .map_err(HttpPrepareError::Transport)?;
                    socket = Some(connected);
                }
            }
            Ok(Box::new(Prepared {
                http,
                request: request.id,
                method: request.method,
                route_hint: request.route,
                managed: intent.is_some(),
                origin,
                deadline,
                route,
                proxy_header,
                socket,
            }) as Box<dyn PreparedHttp>)
        };
        tokio::time::timeout_at(deadline, operation)
            .await
            .map_err(|_| HttpPrepareError::Expired)?
    })
}

impl PreparedHttp for Prepared {
    fn bind(
        mut self: Box<Self>,
        mut packet: HttpRequest,
    ) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        if Instant::now() >= self.deadline {
            return Err(HttpPrepareError::Expired);
        }
        if packet.id != self.request
            || packet.method != self.method
            || packet.route != self.route_hint
            || origin(&packet.url)? != self.origin
        {
            return Err(HttpPrepareError::Invalid(
                "Signed HTTP packet differs from its prepared route",
            ));
        }
        let mut headers: Vec<_> = packet
            .headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .chain(packet.signer_headers.iter().cloned())
            .collect();
        if let Some(route) = &self.route {
            if !route.prefix.is_empty() {
                packet.url = format!("{}/{}", route.prefix, packet.url);
            }
            for (name, value) in &route.headers {
                if self.managed
                    && headers
                        .iter()
                        .any(|(key, previous)| key.eq_ignore_ascii_case(name) && previous != value)
                {
                    return Err(HttpPrepareError::Invalid(
                        "HTTP routing changed a signed header",
                    ));
                }
                insert_header(&mut headers, name.clone(), value.clone());
            }
            route.alias_headers(&mut headers).map_err(|_| {
                HttpPrepareError::Invalid("HTTP header alias conflicts with signed packet")
            })?;
        }
        if let Some((name, value)) = self.proxy_header.take() {
            if self.managed
                && headers
                    .iter()
                    .any(|(key, previous)| key.eq_ignore_ascii_case(&name) && previous != &value)
            {
                return Err(HttpPrepareError::Invalid(
                    "Proxy authorization changed a signed header",
                ));
            }
            insert_header(&mut headers, name, value);
        }
        let proxied =
            self.socket.is_none() && self.route.as_ref().is_some_and(|route| route.authorize);
        let packet = if let Some(socket) = self.socket.take() {
            let packet = http_packet(&packet, &headers)
                .map_err(|_| HttpPrepareError::Invalid("Invalid HTTP tunnel packet"))?;
            Packet::How(Box::new(socket), packet)
        } else {
            let method = match packet.method {
                Method::Get => reqwest::Method::GET,
                Method::Post => reqwest::Method::POST,
                Method::Put => reqwest::Method::PUT,
                Method::Patch => reqwest::Method::PATCH,
                Method::Delete => reqwest::Method::DELETE,
            };
            let mut builder = self
                .http
                .request(method, &packet.url)
                .timeout(self.deadline.saturating_duration_since(Instant::now()));
            for (name, value) in headers {
                builder = builder.header(name, value);
            }
            if let Some(body) = packet.body {
                builder = builder.body(body);
            }
            Packet::Direct(
                self.http.clone(),
                Box::new(
                    builder
                        .build()
                        .map_err(|_| HttpPrepareError::Invalid("Invalid HTTP request packet"))?,
                ),
            )
        };
        Ok(Box::new(Bound {
            request: self.request,
            deadline: self.deadline,
            packet,
            proxied,
        }))
    }
}

impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        Instant::now() < self.deadline
    }

    fn dispatch(self: Box<Self>, events: Events) -> AbortHandle {
        let Self {
            request,
            deadline,
            packet,
            proxied,
        } = *self;
        // These are the last controllable transport API calls. Route/token/HOW
        // preparation and packet validation have all finished before this point.
        let response: Pin<Box<dyn Future<Output = Result<(), String>> + Send>> = match packet {
            Packet::Direct(http, packet) => {
                let response = http.execute(*packet);
                Box::pin(direct_response(response, request, events.clone(), proxied))
            }
            Packet::How(mut socket, packet) => {
                let sent = socket.start_send_unpin(Message::Text(packet.into()));
                let response_events = events.clone();
                Box::pin(async move {
                    sent.map_err(|_| "HTTP tunnel handoff failed".to_owned())?;
                    socket
                        .flush()
                        .await
                        .map_err(|_| "HTTP tunnel handoff failed".to_owned())?;
                    let result = receive_http(&mut socket, request, &response_events).await;
                    let _ = socket.as_mut().close(None).await;
                    result
                })
            }
        };
        tokio::spawn(async move {
            let result = tokio::select! {
                biased;
                _ = events.closed() => return,
                result = tokio::time::timeout_at(deadline, response) => result,
            };
            let error = match result {
                Ok(Ok(())) => return,
                Ok(Err(error)) => error,
                Err(_) => "transport request timed out".into(),
            };
            let _ = events.send(HostEvent::HttpFailed(request, error));
        })
        .abort_handle()
    }
}

const PROXY_AUTHORIZATION_MISSING: &str =
    "Tealstreet proxy authorization missing: no proxy token, so the private request was not sent";

/// The edge's own refusal of our proxy authorization, as opposed to a venue's 401.
fn is_proxy_auth_refusal(body: &str) -> bool {
    let message = serde_json::from_str::<serde_json::Value>(body)
        .ok()
        .and_then(|value| value["message"].as_str().map(str::to_owned))
        .unwrap_or_else(|| body.trim().to_owned());
    message == "Authorization header is missing" || message == "Invalid JWT"
}

async fn direct_response(
    response: impl Future<Output = Result<reqwest::Response, reqwest::Error>>,
    request: RequestId,
    events: Events,
    proxied: bool,
) -> Result<(), String> {
    let response = response
        .await
        .map_err(|error| error.without_url().to_string())?;
    // The edge's 401 fails as a transport error (retried by the venue's read
    // policy), never as a 401 the venue would classify as a refused key.
    if proxied && response.status().as_u16() == 401 {
        let status = response.status().as_u16();
        let headers = response
            .headers()
            .iter()
            .filter_map(|(name, value)| Some(format!("{name}: {}", value.to_str().ok()?)))
            .collect::<Vec<_>>()
            .join("\n");
        let body = response
            .bytes()
            .await
            .map_err(|error| error.without_url().to_string())?;
        let text = String::from_utf8_lossy(&body);
        if is_proxy_auth_refusal(&text) {
            return Err(format!(
                "Tealstreet proxy refused our proxy authorization (HTTP 401: {})",
                text.trim()
            ));
        }
        let _ = events.send(HostEvent::HttpStart {
            request,
            status,
            headers,
        });
        for piece in body.chunks(16 * 1024) {
            let _ = events.send(HostEvent::HttpChunk(request, piece.to_vec()));
        }
        let _ = events.send(HostEvent::HttpEnd(request));
        return Ok(());
    }
    let headers = response
        .headers()
        .iter()
        .filter_map(|(name, value)| Some(format!("{name}: {}", value.to_str().ok()?)))
        .collect::<Vec<_>>()
        .join("\n");
    let _ = events.send(HostEvent::HttpStart {
        request,
        status: response.status().as_u16(),
        headers,
    });
    let mut body = response.bytes_stream();
    while let Some(chunk) = body.next().await {
        let chunk = chunk.map_err(|error| error.without_url().to_string())?;
        for piece in chunk.chunks(16 * 1024) {
            let _ = events.send(HostEvent::HttpChunk(request, piece.to_vec()));
        }
    }
    let _ = events.send(HostEvent::HttpEnd(request));
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{ProxyTokenProvider, RoutingPlan, Transport};
    use std::sync::Arc;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::sync::{mpsc, oneshot};

    fn request(url: &str) -> HttpRequest {
        HttpRequest {
            id: RequestId(17),
            method: Method::Post,
            route: None,
            timeout_ms: Some(5_000),
            url: url.into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        }
    }

    #[tokio::test]
    async fn custom_transport_without_preparation_fails_closed_without_legacy_fallback() {
        struct NoPreparation;
        impl Transport for NoPreparation {
            fn connect(
                &self,
                _: safe_cex_engine::SocketId,
                _: String,
                _: Events,
            ) -> mpsc::UnboundedSender<String> {
                panic!("HTTP preparation must not open a socket")
            }
            fn request(&self, _: HttpRequest, _: Events) {
                panic!("managed preparation must not use the legacy sender")
            }
        }
        let result = NoPreparation
            .prepare_http(
                request("https://venue.invalid/order"),
                RoutingIntent::Primary,
            )
            .await;
        assert!(matches!(result, Err(HttpPrepareError::Unsupported)));
    }

    #[tokio::test]
    async fn prepared_route_binds_exact_signed_bytes_before_synchronous_handoff() {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/private/order", listener.local_addr().unwrap());
        let transport = NetworkTransport::new();
        let packet = request(&url);
        let prepared = transport
            .prepare_http(packet.clone(), RoutingIntent::Primary)
            .await
            .unwrap();
        let mut signed = packet;
        signed
            .url
            .push_str("?timestamp=123&signature=outbound-sentinel");
        signed
            .headers
            .push(("X-Test-Auth", "outbound-header-sentinel".into()));
        signed.body = Some("{ \"amount\": 1.000, \"id\": 18446744073709551615 }".into());
        let bound = prepared.bind(signed).unwrap();
        assert!(bound.ready());
        // A prepared/bound object has no task which can send. Only dispatch
        // below invokes reqwest::Client::execute.
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            let end = loop {
                let mut next = [0; 1024];
                let n = socket.read(&mut next).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&next[..n]);
                if let Some(end) = bytes.windows(4).position(|b| b == b"\r\n\r\n") {
                    break end + 4;
                }
            };
            let headers = String::from_utf8(bytes[..end].to_vec()).unwrap();
            let length: usize = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse().unwrap())
                })
                .unwrap();
            while bytes.len() < end + length {
                let mut next = [0; 1024];
                let n = socket.read(&mut next).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&next[..n]);
            }
            socket
                .write_all(b"HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\n{}")
                .await
                .unwrap();
            String::from_utf8(bytes).unwrap()
        });
        let (events, mut inbox) = mpsc::unbounded_channel();
        let _request_task = bound.dispatch(events);
        let mut body = Vec::new();
        loop {
            match inbox.recv().await.unwrap() {
                HostEvent::HttpStart {
                    request, status, ..
                } => {
                    assert_eq!(request, RequestId(17));
                    assert_eq!(status, 200);
                }
                HostEvent::HttpChunk(_, chunk) => body.extend(chunk),
                HostEvent::HttpEnd(_) => break,
                other => panic!("unexpected request event: {other:?}"),
            }
        }
        assert_eq!(body, b"{}");
        let wire = server.await.unwrap();
        assert!(wire.starts_with(
            "POST /private/order?timestamp=123&signature=outbound-sentinel HTTP/1.1\r\n"
        ));
        assert!(
            wire.to_lowercase()
                .contains("x-test-auth: outbound-header-sentinel\r\n")
        );
        assert!(wire.ends_with("{ \"amount\": 1.000, \"id\": 18446744073709551615 }"));
    }

    #[tokio::test(start_paused = true)]
    async fn proxy_token_wait_and_bound_packet_share_the_original_transport_deadline() {
        let (release, wait) = oneshot::channel();
        let wait = Arc::new(std::sync::Mutex::new(Some(wait)));
        let tokens: ProxyTokenProvider = Arc::new(move || {
            let wait = wait.lock().unwrap().take().unwrap();
            Box::pin(async move { wait.await.map_err(|_| "token cancelled".into()) })
        });
        let transport = NetworkTransport::with_routing(
            RoutingPlan {
                http: HttpRoute {
                    authorize: true,
                    ..HttpRoute::default()
                },
                ..RoutingPlan::default()
            },
            tokens,
        );
        let mut packet = request("https://venue.invalid/private");
        packet.timeout_ms = Some(100);
        let preparation = transport.prepare_http(packet.clone(), RoutingIntent::Primary);
        let pending = tokio::spawn(preparation);
        tokio::task::yield_now().await;
        assert!(!pending.is_finished());
        tokio::time::advance(Duration::from_millis(80)).await;
        release.send("proxy-token-sentinel".into()).unwrap();
        let prepared = pending.await.unwrap().unwrap();
        let bound = prepared.bind(packet).unwrap();
        assert!(bound.ready());
        tokio::time::advance(Duration::from_millis(21)).await;
        assert!(!bound.ready()); // neither signing/binding nor claim resets it
    }

    fn proxied(tokens: ProxyTokenProvider) -> NetworkTransport {
        NetworkTransport::with_routing(
            RoutingPlan {
                http: HttpRoute {
                    authorize: true,
                    ..HttpRoute::default()
                },
                ..RoutingPlan::default()
            },
            tokens,
        )
    }

    /// One HTTP exchange: answers `status` with `body`, returning the request head.
    async fn answer_once(
        status: &str,
        body: &'static str,
    ) -> (String, tokio::task::JoinHandle<String>) {
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let url = format!("http://{}/private/order", listener.local_addr().unwrap());
        let response = format!(
            "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        );
        let server = tokio::spawn(async move {
            let (mut socket, _) = listener.accept().await.unwrap();
            let mut bytes = Vec::new();
            while !bytes.windows(4).any(|b| b == b"\r\n\r\n") {
                let mut next = [0; 1024];
                let n = socket.read(&mut next).await.unwrap();
                assert!(n > 0);
                bytes.extend_from_slice(&next[..n]);
            }
            socket.write_all(response.as_bytes()).await.unwrap();
            String::from_utf8(bytes).unwrap()
        });
        (url, server)
    }

    #[tokio::test]
    async fn a_proxied_request_with_no_token_is_refused_not_sent_bare() {
        let transport = proxied(Arc::new(|| Box::pin(async { Ok(String::new()) })));
        let result = transport
            .prepare_http(
                request("https://venue.invalid/private"),
                RoutingIntent::Primary,
            )
            .await;
        let Err(HttpPrepareError::Transport(message)) = result else {
            panic!("{result:?}")
        };
        assert!(
            message.starts_with("Tealstreet proxy authorization missing"),
            "{message}"
        );
    }

    #[tokio::test]
    async fn the_edges_own_401_fails_as_transport_not_as_a_venue_401() {
        for body in [
            "Authorization header is missing",
            r#"{"message":"Invalid JWT"}"#,
        ] {
            let (url, server) = answer_once("401 Unauthorized", body).await;
            let transport = proxied(Arc::new(|| Box::pin(async { Ok("edge-token".to_owned()) })));
            let packet = request(&url);
            let prepared = transport
                .prepare_http(packet.clone(), RoutingIntent::Primary)
                .await
                .unwrap();
            let (events, mut inbox) = mpsc::unbounded_channel();
            let _task = prepared.bind(packet).unwrap().dispatch(events);
            match inbox.recv().await.unwrap() {
                HostEvent::HttpFailed(request, error) => {
                    assert_eq!(request, RequestId(17));
                    assert!(
                        error.starts_with(
                            "Tealstreet proxy refused our proxy authorization (HTTP 401"
                        ),
                        "{error}"
                    );
                }
                other => panic!("{body}: {other:?}"),
            }
            let head = server.await.unwrap().to_lowercase();
            assert!(
                head.contains("authorization: bearer edge-token\r\n"),
                "{head}"
            );
        }
    }

    #[tokio::test]
    async fn a_venues_own_401_through_the_proxy_reaches_the_engine_unchanged() {
        let venue = r#"{"retCode":10003,"retMsg":"API key is invalid."}"#;
        let (url, _server) = answer_once("401 Unauthorized", venue).await;
        let transport = proxied(Arc::new(|| Box::pin(async { Ok("edge-token".to_owned()) })));
        let packet = request(&url);
        let prepared = transport
            .prepare_http(packet.clone(), RoutingIntent::Primary)
            .await
            .unwrap();
        let (events, mut inbox) = mpsc::unbounded_channel();
        let _task = prepared.bind(packet).unwrap().dispatch(events);
        let mut body = Vec::new();
        loop {
            match inbox.recv().await.unwrap() {
                HostEvent::HttpStart { status, .. } => assert_eq!(status, 401),
                HostEvent::HttpChunk(_, chunk) => body.extend(chunk),
                HostEvent::HttpEnd(_) => break,
                other => panic!("unexpected request event: {other:?}"),
            }
        }
        assert_eq!(body, venue.as_bytes());
    }

    #[tokio::test(start_paused = true)]
    async fn hung_proxy_preparation_expires_without_producing_a_dispatchable_resource() {
        let tokens: ProxyTokenProvider = Arc::new(|| Box::pin(std::future::pending()));
        let transport = NetworkTransport::with_routing(
            RoutingPlan {
                http: HttpRoute {
                    authorize: true,
                    ..HttpRoute::default()
                },
                ..RoutingPlan::default()
            },
            tokens,
        );
        let mut packet = request("https://venue.invalid/private");
        packet.timeout_ms = Some(10);
        let result = transport.prepare_http(packet, RoutingIntent::Primary).await;
        assert!(matches!(result, Err(HttpPrepareError::Expired)));
    }

    #[tokio::test]
    async fn binding_rejects_changed_request_identity_method_route_or_origin() {
        let transport = NetworkTransport::new();
        for change in 0..4 {
            let packet = request("https://venue.invalid/private");
            let prepared = transport
                .prepare_http(packet.clone(), RoutingIntent::Primary)
                .await
                .unwrap();
            let mut changed = packet;
            match change {
                0 => changed.id = RequestId(18),
                1 => changed.method = Method::Get,
                2 => changed.route = Some(RequestRoute::Direct),
                _ => changed.url = "https://other.invalid/private".into(),
            }
            assert!(matches!(
                prepared.bind(changed),
                Err(HttpPrepareError::Invalid(_))
            ));
        }
    }

    #[tokio::test]
    async fn managed_routing_cannot_replace_a_signed_header_or_expose_it_in_debug() {
        let transport = NetworkTransport::with_routing(
            RoutingPlan {
                http: HttpRoute {
                    headers: [("X-Test-Auth".into(), "route-sentinel".into())].into(),
                    ..HttpRoute::default()
                },
                ..RoutingPlan::default()
            },
            Arc::new(|| Box::pin(async { Ok(String::new()) })),
        );
        let mut packet = request("https://venue.invalid/private");
        let prepared = transport
            .prepare_http(packet.clone(), RoutingIntent::Primary)
            .await
            .unwrap();
        assert!(!format!("{prepared:?}").contains("sentinel"));
        packet
            .headers
            .push(("X-Test-Auth", "signed-sentinel".into()));
        assert!(matches!(
            prepared.bind(packet),
            Err(HttpPrepareError::Invalid(_))
        ));

        let transport = NetworkTransport::new();
        let mut packet = request("https://venue.invalid/private");
        let prepared = transport
            .prepare_http(packet.clone(), RoutingIntent::Primary)
            .await
            .unwrap();
        packet
            .headers
            .push(("X-Test-Auth", "signed-sentinel".into()));
        let bound = prepared.bind(packet).unwrap();
        assert!(!format!("{bound:?}").contains("sentinel"));
        assert!(!format!("{bound:?}").contains("venue.invalid"));
    }
}
