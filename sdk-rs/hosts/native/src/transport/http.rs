//! Preparation and final handoff for the existing native HTTP executor.
use super::NetworkTransport;
use crate::how_pool::HowConnection;
use crate::routing::{HttpRoute, HttpRouteMode, http_packet, insert_header};
use crate::{Events, HostEvent};
use futures_util::StreamExt;
use safe_cex_engine::request_budget::RequestClass;
use safe_cex_engine::request_context::RoutingIntent;
use safe_cex_engine::{HttpRequest, Method, RequestId, RequestRoute};
use std::fmt;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;
use tokio::task::AbortHandle;
use tokio::time::Instant;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HttpPrepareError {
    Unsupported,
    Expired,
    ExpiredWithBudget(String),
    Invalid(&'static str),
    Transport(String),
}
impl HttpPrepareError {
    pub fn message(&self) -> &str {
        match self {
            Self::Unsupported => "HTTP transport preparation is unsupported",
            Self::Expired => "transport request timed out",
            Self::ExpiredWithBudget(message) => message,
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
    started: Instant,
    class: RequestClass,
    budget_ms: u32,
    http: reqwest::Client,
    request: RequestId,
    method: Method,
    route_hint: Option<RequestRoute>,
    managed: bool,
    origin: String,
    deadline: Instant,
    route: Option<HttpRoute>,
    proxy_header: Option<(String, String)>,
    tunnel: Option<HowConnection>,
    guard: Option<super::IoGuard>,
    venue: Option<std::sync::Arc<str>>,
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
    How(HowConnection, String),
}
struct Bound {
    started: Instant,
    class: RequestClass,
    budget_ms: u32,
    request: RequestId,
    deadline: Instant,
    packet: Packet,
    /// Sent through the Tealstreet REST proxy, whose own 401 is not the venue's.
    proxied: bool,
    /// Venue host and the way it went, for a transport failure's log line; never a URL.
    route: String,
    guard: Option<super::IoGuard>,
    venue: Option<std::sync::Arc<str>>,
}

/// `api.venue.com direct`, `api.venue.com via proxy.host` or `api.venue.com via HTTP tunnel`.
fn route_label(origin: &str, packet: &Packet) -> String {
    let host = |url: &str| {
        reqwest::Url::parse(url)
            .ok()
            .and_then(|url| url.host_str().map(str::to_owned))
            .unwrap_or_else(|| "unparsable url".to_owned())
    };
    let venue = host(origin);
    match packet {
        Packet::How(..) => format!("{venue} via HTTP tunnel"),
        Packet::Direct(_, request) => match request.url().host_str() {
            Some(sent) if sent != venue => format!("{venue} via {sent}"),
            _ => format!("{venue} direct"),
        },
    }
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
    class: Option<RequestClass>,
) -> HttpPrepareFuture {
    let http = transport.http.clone();
    let routing = transport.routing.clone();
    let guard = transport.io_guard.clone();
    let venue = transport.venue.clone();
    let started = Instant::now();
    let class = class.unwrap_or_else(|| RequestClass::legacy(request.method, &request.url));
    let budget_ms = class.timeout_ms(request.timeout_ms);
    let deadline = started + Duration::from_millis(u64::from(budget_ms));
    let timeout_message = move || {
        format!(
            "HTTP request timed out ({})",
            class.diagnostic(budget_ms, started.elapsed().as_millis())
        )
    };
    Box::pin(async move {
        let operation = async {
            if !super::io_allowed(&guard) {
                return Err(HttpPrepareError::Invalid("account IO authority expired"));
            }
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
            let mut tunnel = None;
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
                    let url = route
                        .tunnel_url
                        .as_deref()
                        .ok_or(HttpPrepareError::Invalid("HOW route missing tunnelUrl"))?;
                    let headers = state
                        .plan
                        .ws_headers
                        .iter()
                        .map(|(name, value)| (name.clone(), value.clone()))
                        .collect();
                    // The account's open socket for this route, opened (or reopened) on demand.
                    let connection = state.how.connection(url, headers, state.tokens.clone());
                    let readiness = deadline.min(started + Duration::from_secs(30));
                    tokio::time::timeout_at(readiness, connection.ready())
                        .await
                        .map_err(|_| {
                            HttpPrepareError::ExpiredWithBudget(format!(
                                "HTTP tunnel readiness timed out ({})",
                                class.diagnostic(budget_ms, started.elapsed().as_millis())
                            ))
                        })?
                        .map_err(|error| {
                            HttpPrepareError::Transport(format!(
                                "{error} ({})",
                                class.diagnostic(budget_ms, started.elapsed().as_millis())
                            ))
                        })?;
                    tunnel = Some(connection);
                }
            }
            Ok(Box::new(Prepared {
                started,
                class,
                budget_ms,
                http,
                request: request.id,
                method: request.method,
                route_hint: request.route,
                managed: intent.is_some(),
                origin,
                deadline,
                route,
                proxy_header,
                tunnel,
                guard,
                venue,
            }) as Box<dyn PreparedHttp>)
        };
        tokio::time::timeout_at(deadline, operation)
            .await
            .map_err(|_| HttpPrepareError::ExpiredWithBudget(timeout_message()))?
    })
}

impl PreparedHttp for Prepared {
    fn bind(
        mut self: Box<Self>,
        mut packet: HttpRequest,
    ) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        if !super::io_allowed(&self.guard) {
            return Err(HttpPrepareError::Invalid("account IO authority expired"));
        }
        if Instant::now() >= self.deadline {
            return Err(HttpPrepareError::ExpiredWithBudget(format!(
                "HTTP request timed out ({})",
                self.class
                    .diagnostic(self.budget_ms, self.started.elapsed().as_millis())
            )));
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
            self.tunnel.is_none() && self.route.as_ref().is_some_and(|route| route.authorize);
        let packet = if let Some(tunnel) = self.tunnel.take() {
            let packet = http_packet(&packet, &headers)
                .map_err(|_| HttpPrepareError::Invalid("Invalid HTTP tunnel packet"))?;
            Packet::How(tunnel, packet)
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
        let route = route_label(&self.origin, &packet);
        Ok(Box::new(Bound {
            started: self.started,
            class: self.class,
            budget_ms: self.budget_ms,
            request: self.request,
            deadline: self.deadline,
            packet,
            proxied,
            route,
            guard: self.guard.clone(),
            venue: self.venue.clone(),
        }))
    }
}

impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        Instant::now() < self.deadline && super::io_allowed(&self.guard)
    }

    fn dispatch(self: Box<Self>, events: Events) -> AbortHandle {
        let Self {
            started: request_started,
            class,
            budget_ms,
            request,
            deadline,
            packet,
            proxied,
            route,
            guard,
            venue,
        } = *self;
        let started = Instant::now();
        // These are the last controllable transport API calls. Route/token/HOW
        // preparation and packet validation have all finished before this point.
        let response_events = events.clone();
        let response = async move {
            if !super::io_allowed(&guard) {
                return Err("account IO authority expired".to_owned());
            }
            match packet {
                Packet::Direct(http, packet) => {
                    direct_response(http.execute(*packet), request, response_events, proxied).await
                }
                Packet::How(tunnel, packet) => {
                    tunnel
                        .request(request, packet, response_events, guard)
                        .await
                }
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
                Err(_) => format!(
                    "HTTP request timed out ({})",
                    class.diagnostic(budget_ms, request_started.elapsed().as_millis())
                ),
            };
            super::log_request_failure(
                &route,
                &format!(
                    "{error} ({})",
                    class.diagnostic(budget_ms, request_started.elapsed().as_millis())
                ),
                started.elapsed(),
                venue.as_deref(),
            );
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

    #[test]
    fn a_failed_request_is_named_by_venue_host_and_route_never_its_url() {
        let client = NetworkTransport::new().http.clone();
        let direct = client
            .get("https://api.bitget.com/api/v2/mix/market/contracts?productType=x")
            .build();
        assert_eq!(
            route_label(
                "https://api.bitget.com",
                &Packet::Direct(client.clone(), Box::new(direct.unwrap()))
            ),
            "api.bitget.com direct"
        );
        let proxied = client
            .get("https://proxy-jp.example/https://api.bitget.com/api/v2/mix/market/contracts?token=t")
            .build()
            .unwrap();
        assert_eq!(
            route_label(
                "https://api.bitget.com",
                &Packet::Direct(client, Box::new(proxied))
            ),
            "api.bitget.com via proxy-jp.example"
        );
    }
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
    async fn io_guard_rechecks_bound_and_scheduled_packets_without_sending_them() {
        use std::sync::atomic::{AtomicBool, Ordering};
        for after_dispatch in [false, true] {
            let (url, server) = answer_once("200 OK", "ok").await;
            let allowed = Arc::new(AtomicBool::new(true));
            let gate = allowed.clone();
            let transport = NetworkTransport::new()
                .with_io_guard(Arc::new(move || gate.load(Ordering::Acquire)));
            let packet = request(&url);
            let prepared = transport
                .prepare_http(packet.clone(), RoutingIntent::Primary)
                .await
                .unwrap();
            let bound = prepared.bind(packet).unwrap();
            assert!(bound.ready());
            let (events, mut replies) = tokio::sync::mpsc::unbounded_channel();
            if after_dispatch {
                bound.dispatch(events);
                allowed.store(false, Ordering::Release);
            } else {
                allowed.store(false, Ordering::Release);
                assert!(!bound.ready());
                bound.dispatch(events);
            }
            let failure = tokio::time::timeout(Duration::from_secs(2), replies.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(
                matches!(failure, HostEvent::HttpFailed(_, ref text) if text == "account IO authority expired")
            );
            assert!(!server.is_finished());
            server.abort();
            assert!(server.await.unwrap_err().is_cancelled());
        }
        // Working control uses the same executor and a real loopback HTTP responder.
        let (url, server) = answer_once("200 OK", "ok").await;
        let transport = NetworkTransport::new().with_io_guard(Arc::new(|| true));
        let packet = request(&url);
        let bound = transport
            .prepare_http(packet.clone(), RoutingIntent::Primary)
            .await
            .unwrap()
            .bind(packet)
            .unwrap();
        let (events, _replies) = tokio::sync::mpsc::unbounded_channel();
        bound.dispatch(events);
        assert!(
            tokio::time::timeout(Duration::from_secs(2), server)
                .await
                .unwrap()
                .unwrap()
                .contains("/private/order")
        );
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
    async fn every_class_bounds_preparation_and_reports_the_original_budget() {
        for class in [
            RequestClass::Discovery,
            RequestClass::OrderRead,
            RequestClass::OrderWrite,
            RequestClass::History,
            RequestClass::Read,
        ] {
            let transport = NetworkTransport::with_routing(
                RoutingPlan {
                    http: HttpRoute {
                        authorize: true,
                        ..HttpRoute::default()
                    },
                    ..RoutingPlan::default()
                },
                Arc::new(|| Box::pin(std::future::pending())),
            );
            let mut packet = request("https://venue.invalid/private?signature=secret-sentinel");
            packet.timeout_ms = None;
            let future = transport.prepare_http_classified(packet, RoutingIntent::Primary, class);
            let task = tokio::spawn(future);
            tokio::task::yield_now().await;
            tokio::time::advance(Duration::from_millis(u64::from(class.budget_ms() - 1))).await;
            assert!(!task.is_finished());
            tokio::time::advance(Duration::from_millis(1)).await;
            let error = task.await.unwrap().unwrap_err();
            assert!(matches!(error, HttpPrepareError::ExpiredWithBudget(_)));
            assert_eq!(
                error.message(),
                format!(
                    "HTTP request timed out ({})",
                    class.diagnostic(class.budget_ms(), u128::from(class.budget_ms()))
                )
            );
            assert!(!error.message().contains("secret-sentinel"));
        }
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

    /// The fleet's route (`PROXY_PRIVATE_URL`) is unauthorized: it admits by network and
    /// forwards every header, so the venue would receive our token.
    #[tokio::test]
    async fn a_fleet_route_sends_no_token_of_ours_and_keeps_the_venues_own_authorization() {
        let fleet = |tokens: ProxyTokenProvider| {
            NetworkTransport::with_routing(
                RoutingPlan {
                    http: HttpRoute {
                        authorize: false,
                        ..HttpRoute::default()
                    },
                    ..RoutingPlan::default()
                },
                tokens,
            )
        };
        for venue_authorization in [None, Some("Bearer venue-jwt")] {
            let (url, server) = answer_once("200 OK", "{}").await;
            let transport = fleet(Arc::new(|| Box::pin(async { Ok("edge-token".to_owned()) })));
            let mut packet = request(&url);
            if let Some(value) = venue_authorization {
                packet.headers.push(("Authorization", value.into()));
            }
            let prepared = transport
                .prepare_http(packet.clone(), RoutingIntent::Primary)
                .await
                .unwrap();
            let (events, mut inbox) = mpsc::unbounded_channel();
            let _task = prepared.bind(packet).unwrap().dispatch(events);
            while !matches!(inbox.recv().await.unwrap(), HostEvent::HttpEnd(_)) {}
            let head = server.await.unwrap().to_lowercase();
            assert!(!head.contains("edge-token"), "{head}");
            match venue_authorization {
                Some(_) => assert!(
                    head.contains("authorization: bearer venue-jwt\r\n"),
                    "{head}"
                ),
                None => assert!(!head.contains("authorization:"), "{head}"),
            }
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
        assert!(matches!(
            result,
            Err(HttpPrepareError::ExpiredWithBudget(_))
        ));
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
