//! Host-only packet paths: applied here, resolved by `resolve` (a port of safe-cex's TS resolver).
use crate::{Events, HostEvent};
use base64::Engine as _;
use futures_util::{SinkExt, StreamExt};
use safe_cex_engine::{HttpRequest, RequestId};
use serde::{Deserialize, Serialize};
use serde_json::value::RawValue;
use std::collections::{BTreeMap, HashMap};
use std::future::Future;
use std::io::Read;
use std::pin::Pin;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::net::TcpStream;
use tokio_tungstenite::{MaybeTlsStream, WebSocketStream, tungstenite::Message};

pub mod resolve;

pub type ProxyTokenFuture = Pin<Box<dyn Future<Output = Result<String, String>> + Send>>;
/// Resolve a current account-context token for each operation; never put JWTs in a plan.
pub type ProxyTokenProvider = Arc<dyn Fn() -> ProxyTokenFuture + Send + Sync>;

#[derive(Clone, Copy, Debug, Default, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum HttpRouteMode {
    #[default]
    Direct,
    How,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct HttpRoute {
    pub mode: HttpRouteMode,
    pub prefix: String,
    pub authorize: bool,
    pub authorization_header: String,
    pub headers: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub header_aliases: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tunnel_url: Option<String>,
}

impl Default for HttpRoute {
    fn default() -> Self {
        Self {
            mode: HttpRouteMode::Direct,
            prefix: String::new(),
            authorize: false,
            authorization_header: "Authorization".into(),
            headers: BTreeMap::new(),
            header_aliases: BTreeMap::new(),
            tunnel_url: None,
        }
    }
}

impl HttpRoute {
    pub(crate) fn alias_headers(&self, headers: &mut Vec<(String, String)>) -> Result<(), String> {
        for (source, target) in &self.header_aliases {
            if source.eq_ignore_ascii_case(target) {
                continue;
            }
            let Some(value) = headers
                .iter()
                .find(|(name, _)| name.eq_ignore_ascii_case(source))
                .map(|(_, value)| value.clone())
            else {
                continue;
            };
            if headers
                .iter()
                .any(|(name, previous)| name.eq_ignore_ascii_case(target) && *previous != value)
            {
                return Err("Engine HTTP header alias conflicts with an existing header".into());
            }
            headers.retain(|(name, _)| !name.eq_ignore_ascii_case(source));
            insert_header(headers, target.clone(), value);
        }
        Ok(())
    }
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct WsRoute {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fallback: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub relay: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub initial_silent_connects: Option<usize>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub ignored_messages: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub direct_urls: Vec<String>,
}

#[derive(Clone, Debug, Default, Deserialize, Serialize)]
pub struct WsRoutes {
    pub public: WsRoute,
    pub private: WsRoute,
    pub business: WsRoute,
}

#[derive(Clone, Copy, Debug, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum SocketChannel {
    Public,
    Private,
    Business,
}

#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct SocketChannelRule {
    pub contains: String,
    pub channel: SocketChannel,
}

/// A public socket whose native URL names a product with its own route.
#[derive(Clone, Debug, Deserialize, Serialize)]
pub struct PublicWsRoute {
    pub contains: String,
    pub route: WsRoute,
}

/// Serializable output of TS resolution. This host has no venue routing table.
#[derive(Clone, Debug, Default, Deserialize, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RoutingPlan {
    pub http: HttpRoute,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_http: Option<HttpRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_http: Option<HttpRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_public_http: Option<HttpRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub read_header: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub read_public: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_cors_http: Option<HttpRoute>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub public_header: Option<String>,
    pub ws: WsRoutes,
    pub ws_headers: BTreeMap<String, String>,
    #[serde(default)]
    pub venue_ws_headers: BTreeMap<String, String>,
    pub socket_channels: Vec<SocketChannelRule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub public_ws_routes: Vec<PublicWsRoute>,
    /// Private sockets whose native URL has its own route (Bitget UTA).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub private_ws_routes: Vec<PublicWsRoute>,
}

pub(crate) struct RoutingState {
    pub plan: RoutingPlan,
    pub tokens: ProxyTokenProvider,
    sockets: Mutex<HashMap<String, SocketAttempts>>,
}

#[derive(Default)]
struct SocketAttempts {
    connected: bool,
    message: bool,
    silent: usize,
    candidate: usize,
    ignored_messages: Vec<String>,
}

pub(crate) struct SelectedSocket {
    pub url: String,
    pub headers: Vec<(String, String)>,
    pub key: String,
    pub relay: Option<String>,
    /// A `reject_route` connect with no route left: report, never dial.
    pub exhausted: bool,
}

impl RoutingState {
    pub fn new(plan: RoutingPlan, tokens: ProxyTokenProvider) -> Self {
        Self {
            plan,
            tokens,
            sockets: Mutex::new(HashMap::new()),
        }
    }

    pub fn http(&self, request: &HttpRequest) -> Result<HttpRoute, String> {
        if let Some(route) = self.explicit_http(request.route)? {
            return Ok(route);
        }
        let public = self.plan.public_header.as_ref().is_some_and(|header| {
            !request
                .headers
                .iter()
                .any(|(name, value)| name.eq_ignore_ascii_case(header) && !value.is_empty())
        });
        let read = request.method == safe_cex_engine::Method::Get
            && (self.plan.read_public
                || self
                    .plan
                    .read_header
                    .as_ref()
                    .or(self.plan.public_header.as_ref())
                    .is_some_and(|header| {
                        request.headers.iter().any(|(name, value)| {
                            name.eq_ignore_ascii_case(header) && !value.is_empty()
                        })
                    }));
        Ok(
            if let Some(route) = self.plan.read_http.as_ref().filter(|_| read) {
                if public {
                    self.plan.read_public_http.as_ref().unwrap_or(route)
                } else {
                    route
                }
            } else if public {
                self.plan.public_http.as_ref().unwrap_or(&self.plan.http)
            } else {
                &self.plan.http
            }
            .clone(),
        )
    }

    /// Managed unsigned requests declare their identity before key presentation.
    /// This path never inspects headers or uses GET as credential policy.
    pub fn managed_http(
        &self,
        request: &HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> Result<HttpRoute, String> {
        use safe_cex_engine::request_context::RoutingIntent;
        if request.route == Some(safe_cex_engine::RequestRoute::Primary)
            && intent == RoutingIntent::Secondary
        {
            return Err("Secondary credentials conflict with primary HTTP routing".into());
        }
        if request.route == Some(safe_cex_engine::RequestRoute::Secondary)
            && intent != RoutingIntent::Secondary
        {
            return Err("Secondary HTTP routing requires secondary credentials".into());
        }
        // A declared secondary route still requires a configured read egress.
        if let Some(route) = self
            .explicit_http(request.route)?
            .filter(|_| request.route != Some(safe_cex_engine::RequestRoute::Secondary))
        {
            return Ok(route);
        }
        match intent {
            RoutingIntent::Primary => Ok(self.plan.http.clone()),
            RoutingIntent::Secondary => self
                .plan
                .read_http
                .clone()
                .ok_or_else(|| "Secondary HTTP routing is not configured".into()),
            RoutingIntent::Public => Ok(if self.plan.read_public {
                self.plan
                    .read_public_http
                    .as_ref()
                    .or(self.plan.read_http.as_ref())
                    .or(self.plan.public_http.as_ref())
                    .unwrap_or(&self.plan.http)
            } else {
                self.plan.public_http.as_ref().unwrap_or(&self.plan.http)
            }
            .clone()),
        }
    }

    fn explicit_http(
        &self,
        route: Option<safe_cex_engine::RequestRoute>,
    ) -> Result<Option<HttpRoute>, String> {
        use safe_cex_engine::RequestRoute;
        match route {
            Some(RequestRoute::Primary) => Ok(Some(self.plan.http.clone())),
            // Same fallback as an inferred read when no read egress is configured.
            Some(RequestRoute::Secondary) => Ok(Some(
                self.plan
                    .read_http
                    .as_ref()
                    .unwrap_or(&self.plan.http)
                    .clone(),
            )),
            Some(RequestRoute::Direct) => Ok(Some(HttpRoute::default())),
            Some(RequestRoute::PublicCors) => self
                .plan
                .public_cors_http
                .clone()
                .map(Some)
                .ok_or_else(|| "Public CORS request routing is not configured".into()),
            None => Ok(None),
        }
    }

    pub fn socket(
        &self,
        original: &str,
        mut headers: Vec<(String, String)>,
        explicit: Option<safe_cex_engine::SocketChannel>,
        context: Option<safe_cex_engine::SocketContext>,
    ) -> SelectedSocket {
        for (name, value) in &self.plan.venue_ws_headers {
            insert_header(&mut headers, name.clone(), value.clone());
        }
        let inferred = self
            .plan
            .socket_channels
            .iter()
            .find(|rule| original.contains(&rule.contains))
            .map_or(SocketChannel::Public, |rule| rule.channel);
        let channel = explicit.map_or(inferred, |channel| match channel {
            safe_cex_engine::SocketChannel::Public => SocketChannel::Public,
            safe_cex_engine::SocketChannel::Private => SocketChannel::Private,
            safe_cex_engine::SocketChannel::Business => SocketChannel::Business,
        });
        let route = match channel {
            SocketChannel::Public => self
                .plan
                .public_ws_routes
                .iter()
                .find(|rule| original.contains(&rule.contains))
                .map_or(&self.plan.ws.public, |rule| &rule.route),
            SocketChannel::Private => self
                .plan
                .private_ws_routes
                .iter()
                .find(|rule| original.contains(&rule.contains))
                .map_or(&self.plan.ws.private, |rule| &rule.route),
            SocketChannel::Business => &self.plan.ws.business,
        };
        let feed = context.as_ref().map_or("", |context| context.feed.as_str());
        let recover = context.as_ref().is_some_and(|context| context.recover);
        let reject = context.as_ref().is_some_and(|context| context.reject_route);
        let key = format!(
            "{channel:?}:{feed}:{}",
            original.split('?').next().unwrap_or(original)
        );
        if route.direct_urls.iter().any(|url| url == original) {
            return SelectedSocket {
                url: original.into(),
                headers,
                key: format!("direct:{key}"),
                relay: None,
                // A direct-only route has no other egress to try.
                exhausted: reject,
            };
        }
        let mut sockets = self.sockets.lock().unwrap();
        let attempts = sockets
            .entry(key.clone())
            .or_insert_with(|| SocketAttempts {
                silent: route.initial_silent_connects.unwrap_or(0),
                ignored_messages: route.ignored_messages.clone(),
                ..SocketAttempts::default()
            });
        // A refusal is a reply, never silence.
        if attempts.connected && !reject {
            attempts.silent = if attempts.message {
                0
            } else {
                attempts.silent.saturating_add(1)
            };
        }
        attempts.connected = true;
        attempts.message = false;
        let fallback = route.fallback.as_deref().filter(|url| !url.is_empty());
        let mut candidates = vec![route.host.as_deref().unwrap_or(original)];
        candidates.extend(
            [route.relay.as_deref(), fallback]
                .into_iter()
                .flatten()
                .filter(|url| !url.is_empty()),
        );
        if reject {
            // Advance exactly one route; `recover` does not also jump to the last.
            if attempts.candidate + 1 >= candidates.len() {
                return SelectedSocket {
                    url: original.into(),
                    headers,
                    key,
                    relay: None,
                    exhausted: true,
                };
            }
            attempts.candidate += 1;
            attempts.silent = 0;
        } else if recover && fallback.is_some() {
            attempts.candidate = candidates.len() - 1;
            attempts.silent = 0;
        } else if attempts.silent >= 2 && attempts.candidate + 1 < candidates.len() {
            attempts.candidate += 1;
            attempts.silent = 0;
        }
        let mut url = candidates[attempts.candidate.min(candidates.len() - 1)].to_owned();
        if url != original {
            for (name, value) in &self.plan.ws_headers {
                insert_header(&mut headers, name.clone(), value.clone());
            }
        }
        let relay = reqwest::Url::parse(&url).ok().and_then(|mut parsed| {
            let upstream = parsed
                .query_pairs()
                .find(|(name, _)| name == "ts-relay")?
                .1
                .into_owned();
            let retained: Vec<_> = parsed
                .query_pairs()
                .filter(|(name, _)| name != "ts-relay")
                .map(|(name, value)| (name.into_owned(), value.into_owned()))
                .collect();
            parsed.set_query(None);
            if !retained.is_empty() {
                parsed.query_pairs_mut().extend_pairs(retained);
            }
            url = parsed.to_string();
            Some(upstream)
        });
        SelectedSocket {
            url,
            headers,
            key,
            relay,
            exhausted: false,
        }
    }

    pub fn binary_message(&self, key: &str) {
        if let Some(attempts) = self.sockets.lock().unwrap().get_mut(key) {
            attempts.message = true;
        }
    }

    pub fn message(&self, key: &str, text: &str) {
        if let Some(attempts) = self.sockets.lock().unwrap().get_mut(key)
            && !attempts
                .ignored_messages
                .iter()
                .any(|ignored| ignored == text)
        {
            attempts.message = true;
        }
    }
}

pub(crate) fn insert_header(headers: &mut Vec<(String, String)>, name: String, value: String) {
    headers.retain(|(key, _)| !key.eq_ignore_ascii_case(&name));
    headers.push((name, value));
}

pub(crate) type Socket = WebSocketStream<MaybeTlsStream<TcpStream>>;

#[derive(Deserialize)]
struct TunnelMessage {
    #[serde(rename = "type")]
    kind: String,
    #[serde(default)]
    nonce: Option<String>,
    #[serde(default)]
    status: Option<String>,
    #[serde(default)]
    error: Option<Box<RawValue>>,
    #[serde(default)]
    data: Option<TunnelResponse>,
}

#[derive(Deserialize)]
struct TunnelResponse {
    #[serde(default)]
    status: Option<u16>,
    #[serde(default)]
    error: Option<Box<RawValue>>,
    #[serde(default)]
    headers: BTreeMap<String, TunnelHeaderValue>,
    #[serde(default)]
    data_raw: Option<String>,
    #[serde(default)]
    data_encoded: Option<String>,
    #[serde(default)]
    data_encoding: Option<String>,
    #[serde(default)]
    data: Option<Box<RawValue>>,
}

/// Match the JS host's String(value), including Array#toString's null entries.
#[derive(Deserialize)]
#[serde(untagged)]
enum TunnelHeaderValue {
    Text(String),
    Array(Vec<TunnelHeaderValue>),
    Boolean(bool),
    Number(f64),
    Null(()),
    Object(BTreeMap<String, serde::de::IgnoredAny>),
}

impl TunnelHeaderValue {
    fn js_string(&self) -> String {
        match self {
            Self::Text(text) => text.clone(),
            Self::Array(values) => values
                .iter()
                .map(|value| match value {
                    Self::Null(()) => String::new(),
                    value => value.js_string(),
                })
                .collect::<Vec<_>>()
                .join(","),
            Self::Boolean(value) => value.to_string(),
            Self::Number(value) if *value == 0.0 => "0".into(),
            Self::Number(value) if value.abs() >= 1e21 || value.abs() < 1e-6 => {
                let scientific = format!("{value:e}");
                let (mantissa, exponent) = scientific.split_once('e').unwrap();
                let sign = if exponent.starts_with('-') { "" } else { "+" };
                format!("{mantissa}e{sign}{exponent}")
            }
            Self::Number(value) => value.to_string(),
            Self::Null(()) => "null".into(),
            Self::Object(fields) => {
                // Deserialize object members without retaining their values.
                let _ = fields;
                "[object Object]".into()
            }
        }
    }
}

#[derive(Serialize)]
struct Auth<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    token: &'a str,
    source: &'static str,
}

#[derive(Serialize)]
struct Relay<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    upstream: &'a str,
}

#[derive(Serialize)]
struct Ping {
    #[serde(rename = "type")]
    kind: &'static str,
    nonce: &'static str,
}

#[derive(Serialize)]
struct HttpRpc<'a> {
    #[serde(rename = "type")]
    kind: &'static str,
    rpc_version: u8,
    format: &'static str,
    accept_encoding: [&'static str; 1],
    accept_binary: bool,
    nonce: &'a str,
    arg: HttpArgument<'a>,
}

#[derive(Serialize)]
struct HttpArgument<'a> {
    url: &'a str,
    method: &'static str,
    headers: BTreeMap<&'a str, &'a str>,
    params: BTreeMap<&'static str, &'static str>,
    data: Box<RawValue>,
}

async fn send(socket: &mut Socket, message: &impl Serialize) -> Result<(), String> {
    let text = serde_json::to_string(message).map_err(|error| error.to_string())?;
    socket
        .send(Message::text(text))
        .await
        .map_err(|error| error.to_string())
}

/// Return only protocol data; websocket ping frames are answered while waiting.
async fn receive(socket: &mut Socket) -> Result<(TunnelMessage, Option<Vec<u8>>), String> {
    loop {
        match socket.next().await {
            Some(Ok(Message::Text(text))) => {
                return serde_json::from_str(&text)
                    .map(|message| (message, None))
                    .map_err(|error| error.to_string());
            }
            Some(Ok(Message::Binary(bytes))) => return decode_binary(&bytes),
            Some(Ok(Message::Ping(_))) => {
                socket.flush().await.map_err(|error| error.to_string())?
            }
            Some(Ok(Message::Close(_))) | None => return Err("tunnel closed".into()),
            Some(Err(error)) => return Err(error.to_string()),
            Some(Ok(_)) => {}
        }
    }
}

fn decode_binary(bytes: &[u8]) -> Result<(TunnelMessage, Option<Vec<u8>>), String> {
    if bytes.len() < 8 || &bytes[..4] != b"TSB1" {
        return Err("invalid tunnel binary prefix".into());
    }
    let header_len = u32::from_be_bytes(bytes[4..8].try_into().unwrap()) as usize;
    if header_len == 0 || header_len > 1024 * 1024 || 8 + header_len >= bytes.len() {
        return Err("invalid tunnel binary header length".into());
    }
    let message: TunnelMessage =
        serde_json::from_slice(&bytes[8..8 + header_len]).map_err(|error| error.to_string())?;
    if message.kind != "rpc.http"
        || message.nonce.is_none()
        || !message
            .data
            .as_ref()
            .is_some_and(|data| data.data_encoding.as_deref() == Some("gzip"))
    {
        return Err("invalid tunnel binary response header".into());
    }
    Ok((message, Some(bytes[8 + header_len..].to_vec())))
}

async fn ping(socket: &mut Socket) -> Result<(), String> {
    send(
        socket,
        &Ping {
            kind: "rpc.ping",
            nonce: "native-host",
        },
    )
    .await
}

async fn send_current_token(
    socket: &mut Socket,
    tokens: &ProxyTokenProvider,
) -> Result<(), String> {
    loop {
        let token = tokens()
            .await
            .map_err(|_| "Proxy authorization failed".to_owned())?;
        if !token.is_empty() {
            return send(
                socket,
                &Auth {
                    kind: "rpc.auth",
                    token: &token,
                    source: "worker",
                },
            )
            .await;
        }
        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

/// HOW waits for the server greeting and honors IP-whitelist authentication.
pub(crate) async fn authenticate_how(
    socket: &mut Socket,
    tokens: &ProxyTokenProvider,
) -> Result<(), String> {
    let mut auth_sent = false;
    let mut rejected = 0;
    loop {
        let (message, _) = receive(socket).await?;
        match message.kind.as_str() {
            "rpc.auth" if message.error.is_none() => return Ok(()),
            "rpc.auth" => {
                rejected += 1;
                if rejected > 10 {
                    return Err("tunnel authentication rejected".into());
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
                send_current_token(socket, tokens).await?;
                auth_sent = true;
            }
            "serverTime" if !auth_sent => {
                // HOW's existing adapter polls at 25 ms; let automatic auth arrive first.
                tokio::task::yield_now().await;
                let pending =
                    tokio::time::timeout(Duration::from_millis(25), receive(socket)).await;
                if let Ok(Ok((message, _))) = pending {
                    if message.kind == "rpc.auth" && message.error.is_none() {
                        return Ok(());
                    }
                    if message.kind == "heartbeat" {
                        ping(socket).await?;
                    }
                }
                send_current_token(socket, tokens).await?;
                auth_sent = true;
            }
            "heartbeat" => ping(socket).await?,
            _ => {}
        }
    }
}

pub(crate) async fn authenticate_relay(
    socket: &mut Socket,
    tokens: &ProxyTokenProvider,
    upstream: &str,
) -> Result<(), String> {
    let token = tokens().await?;
    send(
        socket,
        &Auth {
            kind: "rpc.auth",
            token: &token,
            source: "worker",
        },
    )
    .await?;
    let mut relay_sent = false;
    loop {
        let (message, _) = receive(socket).await?;
        match message.kind.as_str() {
            "rpc.auth" if message.status.as_deref() == Some("success") && !relay_sent => {
                send(
                    socket,
                    &Relay {
                        kind: "rpc.relay",
                        upstream,
                    },
                )
                .await?;
                relay_sent = true;
            }
            "rpc.auth" if message.status.as_deref() != Some("success") => {
                return Err("tunnel authentication rejected".into());
            }
            "rpc.relay" if relay_sent && message.status.as_deref() == Some("open") => return Ok(()),
            "rpc.relay" => return Err("tunnel relay refused".into()),
            "heartbeat" => ping(socket).await?,
            _ => {}
        }
    }
}

fn argument_body(body: Option<&str>) -> Result<Box<RawValue>, String> {
    let json = match body {
        Some(body) => serde_json::to_string(body).map_err(|error| error.to_string())?,
        None => "{}".into(),
    };
    RawValue::from_string(json).map_err(|error| error.to_string())
}

pub(crate) fn http_packet(
    request: &HttpRequest,
    headers: &[(String, String)],
) -> Result<String, String> {
    let nonce = format!("native-{}", request.id.0);
    serde_json::to_string(&HttpRpc {
        kind: "rpc.http",
        rpc_version: 4,
        format: "raw",
        accept_encoding: ["gzip"],
        accept_binary: true,
        nonce: &nonce,
        arg: HttpArgument {
            url: &request.url,
            method: request.method.as_str(),
            headers: headers
                .iter()
                .map(|(name, value)| (name.as_str(), value.as_str()))
                .collect(),
            params: BTreeMap::new(),
            data: argument_body(request.body.as_deref())?,
        },
    })
    .map_err(|_| "Invalid HTTP tunnel packet".into())
}

pub(crate) async fn receive_http(
    socket: &mut Socket,
    request: RequestId,
    events: &Events,
) -> Result<(), String> {
    let nonce = format!("native-{}", request.0);
    loop {
        let (message, binary) = receive(socket).await?;
        if message.kind == "heartbeat" {
            ping(socket).await?;
        }
        if message.kind != "rpc.http" || message.nonce.as_deref() != Some(&nonce) {
            continue;
        }
        let response = message.data.ok_or("tunnel response missing data")?;
        if response.error.is_some() {
            return Err("tunnel request failed".into());
        }
        let status = response.status.ok_or("tunnel response missing status")?;
        let mut response_headers = Vec::new();
        for (name, value) in response.headers {
            response_headers.push(format!("{name}: {}", value.js_string()));
        }
        let body = if let Some(binary) = binary {
            gunzip(&binary)?
        } else if response.data_encoding.as_deref() == Some("gzip+base64") {
            let bytes = base64::engine::general_purpose::STANDARD
                .decode(
                    response
                        .data_encoded
                        .ok_or("tunnel response missing encoded body")?,
                )
                .map_err(|error| error.to_string())?;
            gunzip(&bytes)?
        } else if let Some(raw) = response.data_raw {
            raw.into_bytes()
        } else if let Some(data) = response.data {
            serde_json::from_str::<String>(data.get())
                .map(String::into_bytes)
                .map_err(|_| "tunnel response has no raw body".to_owned())?
        } else {
            return Err("tunnel response has no raw body".into());
        };
        report_response(request, status, response_headers.join("\n"), &body, events);
        return Ok(());
    }
}

fn gunzip(bytes: &[u8]) -> Result<Vec<u8>, String> {
    let mut body = Vec::new();
    flate2::read::GzDecoder::new(bytes)
        .read_to_end(&mut body)
        .map_err(|error| error.to_string())?;
    Ok(body)
}

fn report_response(request: RequestId, status: u16, headers: String, body: &[u8], events: &Events) {
    let _ = events.send(HostEvent::HttpStart {
        request,
        status,
        headers,
    });
    for chunk in body.chunks(16 * 1024) {
        let _ = events.send(HostEvent::HttpChunk(request, chunk.to_vec()));
    }
    let _ = events.send(HostEvent::HttpEnd(request));
}

#[cfg(test)]
mod tests {
    use super::*;
    use safe_cex_engine::{Method, RequestId};

    fn state(plan: RoutingPlan) -> RoutingState {
        RoutingState::new(plan, Arc::new(|| Box::pin(async { Ok(String::new()) })))
    }

    #[test]
    fn managed_unsigned_routing_uses_explicit_identity_even_for_post_reads() {
        use safe_cex_engine::request_context::RoutingIntent;
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "primary".into(),
                ..HttpRoute::default()
            },
            public_http: Some(HttpRoute {
                prefix: "public".into(),
                ..HttpRoute::default()
            }),
            read_http: Some(HttpRoute {
                prefix: "secondary".into(),
                ..HttpRoute::default()
            }),
            public_header: Some("X-Account-Auth".into()),
            read_header: Some("X-Read-Auth".into()),
            ..RoutingPlan::default()
        });
        let mut packet = HttpRequest {
            id: RequestId(1),
            method: Method::Post,
            route: None,
            timeout_ms: None,
            url: "https://venue.invalid/account".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(
            state
                .managed_http(&packet, RoutingIntent::Public)
                .unwrap()
                .prefix,
            "public"
        );
        assert_eq!(
            state
                .managed_http(&packet, RoutingIntent::Primary)
                .unwrap()
                .prefix,
            "primary"
        );
        assert_eq!(
            state
                .managed_http(&packet, RoutingIntent::Secondary)
                .unwrap()
                .prefix,
            "secondary"
        );
        packet
            .headers
            .push(("X-Account-Auth", "outbound-sentinel".into()));
        packet
            .headers
            .push(("X-Read-Auth", "outbound-sentinel".into()));
        packet.method = Method::Get;
        assert_eq!(
            state
                .managed_http(&packet, RoutingIntent::Public)
                .unwrap()
                .prefix,
            "public"
        );
        assert_eq!(
            state
                .managed_http(&packet, RoutingIntent::Primary)
                .unwrap()
                .prefix,
            "primary"
        );
        packet.route = Some(safe_cex_engine::RequestRoute::Primary);
        assert!(
            state
                .managed_http(&packet, RoutingIntent::Secondary)
                .is_err()
        );
    }

    #[test]
    fn managed_secondary_route_does_not_silently_fall_back_to_primary() {
        use safe_cex_engine::request_context::RoutingIntent;
        let state = state(RoutingPlan::default());
        let packet = HttpRequest {
            id: RequestId(1),
            method: Method::Post,
            route: None,
            timeout_ms: None,
            url: "https://venue.invalid/account".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert!(
            state
                .managed_http(&packet, RoutingIntent::Secondary)
                .is_err()
        );
    }

    #[test]
    fn empty_or_absent_header_uses_public_route_case_insensitively() {
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "private".into(),
                ..HttpRoute::default()
            },
            public_http: Some(HttpRoute {
                prefix: "public".into(),
                ..HttpRoute::default()
            }),
            public_header: Some("X-MBX-APIKEY".into()),
            ..RoutingPlan::default()
        });
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: "https://venue.invalid".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(state.http(&request).unwrap().prefix, "public");
        request.headers.push(("x-mbx-apikey", String::new()));
        assert_eq!(state.http(&request).unwrap().prefix, "public");
        request.headers[0].1 = "venue-key".into();
        assert_eq!(state.http(&request).unwrap().prefix, "private");
    }

    #[test]
    fn unsigned_get_uses_declared_dynamic_client_but_primary_hint_and_post_do_not() {
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "primary".into(),
                ..HttpRoute::default()
            },
            read_http: Some(HttpRoute {
                prefix: "secondary".into(),
                ..HttpRoute::default()
            }),
            read_public: true,
            ..RoutingPlan::default()
        });
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: "https://venue.test/catalog".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(state.http(&request).unwrap().prefix, "secondary");
        request.route = Some(safe_cex_engine::RequestRoute::Primary);
        assert_eq!(state.http(&request).unwrap().prefix, "primary");
        request.route = None;
        request.method = Method::Post;
        assert_eq!(state.http(&request).unwrap().prefix, "primary");
    }

    #[test]
    fn secondary_public_route_preserves_policy_after_credentials_are_removed() {
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "primary-private".into(),
                ..HttpRoute::default()
            },
            public_http: Some(HttpRoute {
                prefix: "primary-public".into(),
                ..HttpRoute::default()
            }),
            read_http: Some(HttpRoute {
                prefix: "read-private".into(),
                ..HttpRoute::default()
            }),
            read_public_http: Some(HttpRoute {
                prefix: "read-public".into(),
                ..HttpRoute::default()
            }),
            public_header: Some("X-MBX-APIKEY".into()),
            read_header: Some("X-MBX-APIKEY".into()),
            read_public: true,
            ..RoutingPlan::default()
        });
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: "https://venue.test/catalog".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(state.http(&request).unwrap().prefix, "read-public");
        request.headers.push(("x-mbx-apikey", "read-key".into()));
        assert_eq!(state.http(&request).unwrap().prefix, "read-private");
        request.route = Some(safe_cex_engine::RequestRoute::Primary);
        assert_eq!(state.http(&request).unwrap().prefix, "primary-private");
        request.route = None;
        request.method = Method::Post;
        assert_eq!(state.http(&request).unwrap().prefix, "primary-private");
        request.headers.clear();
        assert_eq!(state.http(&request).unwrap().prefix, "primary-public");
    }

    #[test]
    fn secondary_read_route_applies_only_to_signed_gets() {
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "primary".into(),
                ..HttpRoute::default()
            },
            read_http: Some(HttpRoute {
                prefix: "secondary-cors".into(),
                mode: HttpRouteMode::How,
                tunnel_url: Some("wss://read-tunnel.example".into()),
                ..HttpRoute::default()
            }),
            public_http: Some(HttpRoute {
                prefix: "public".into(),
                ..HttpRoute::default()
            }),
            public_header: Some("X-MBX-APIKEY".into()),
            ..RoutingPlan::default()
        });
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Get,
            route: None,
            timeout_ms: None,
            url: "https://fapi.asterdex.com/fapi/v3/account?signature=exact".into(),
            headers: vec![("x-mbx-apikey", "read-wallet".into())],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(state.http(&request).unwrap().prefix, "secondary-cors");
        assert_eq!(state.http(&request).unwrap().mode, HttpRouteMode::How);
        request.route = Some(safe_cex_engine::RequestRoute::Primary);
        assert_eq!(state.http(&request).unwrap().prefix, "primary");
        request.route = None;
        for method in [Method::Post, Method::Put, Method::Delete] {
            request.method = method;
            assert_eq!(state.http(&request).unwrap().prefix, "primary");
        }
        request.method = Method::Get;
        request.headers.clear();
        assert_eq!(state.http(&request).unwrap().prefix, "public");
        request.headers.push(("X-MBX-APIKEY", String::new()));
        assert_eq!(state.http(&request).unwrap().prefix, "public");
    }

    #[test]
    fn declared_secondary_route_uses_read_egress_regardless_of_method() {
        let plan = RoutingPlan {
            http: HttpRoute {
                prefix: "primary".into(),
                ..HttpRoute::default()
            },
            read_http: Some(HttpRoute {
                prefix: "read".into(),
                ..HttpRoute::default()
            }),
            ..RoutingPlan::default()
        };
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Post,
            route: Some(safe_cex_engine::RequestRoute::Secondary),
            timeout_ms: None,
            url: "https://venue.test/account".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        assert_eq!(state(plan.clone()).http(&request).unwrap().prefix, "read");
        let unconfigured = state(RoutingPlan {
            read_http: None,
            ..plan.clone()
        });
        assert_eq!(unconfigured.http(&request).unwrap().prefix, "primary");
        use safe_cex_engine::request_context::RoutingIntent;
        request.method = Method::Get;
        let state = state(plan);
        assert_eq!(
            state
                .managed_http(&request, RoutingIntent::Secondary)
                .unwrap()
                .prefix,
            "read"
        );
        for intent in [RoutingIntent::Primary, RoutingIntent::Public] {
            assert!(state.managed_http(&request, intent).is_err());
        }
        assert!(
            unconfigured
                .managed_http(&request, RoutingIntent::Secondary)
                .is_err()
        );
    }

    #[test]
    fn public_request_hints_bypass_account_routing_and_fail_closed_when_unconfigured() {
        let state = state(RoutingPlan {
            http: HttpRoute {
                prefix: "account-proxy".into(),
                authorize: true,
                ..HttpRoute::default()
            },
            public_cors_http: Some(HttpRoute {
                prefix: "https://public-cors.test".into(),
                ..HttpRoute::default()
            }),
            ..RoutingPlan::default()
        });
        let mut request = HttpRequest {
            id: RequestId(0),
            method: Method::Get,
            route: Some(safe_cex_engine::RequestRoute::Direct),
            timeout_ms: None,
            url: "https://venue.test/time".into(),
            headers: vec![],
            signer_headers: vec![],
            body: None,
        };
        let direct = state.http(&request).unwrap();
        assert!(direct.prefix.is_empty());
        assert!(!direct.authorize);
        assert!(direct.headers.is_empty());
        assert_eq!(direct.mode, HttpRouteMode::Direct);
        request.route = Some(safe_cex_engine::RequestRoute::PublicCors);
        let cors = state.http(&request).unwrap();
        assert_eq!(cors.prefix, "https://public-cors.test");
        assert!(!cors.authorize);
        assert!(cors.headers.is_empty());
        assert_eq!(cors.mode, HttpRouteMode::Direct);
        let absent = RoutingState::new(
            RoutingPlan::default(),
            Arc::new(|| Box::pin(async { Ok(String::new()) })),
        );
        assert!(
            absent
                .http(&request)
                .unwrap_err()
                .contains("not configured")
        );
        request.route = None;
        assert_eq!(state.http(&request).unwrap().prefix, "account-proxy");
    }

    #[test]
    fn native_plan_json_round_trips_the_js_schema() {
        let json = r#"{"http":{"mode":"direct","prefix":"","authorize":false,"authorizationHeader":"Authorization","headers":{}},"ws":{"public":{},"private":{},"business":{}},"wsHeaders":{},"socketChannels":[]}"#;
        let plan: RoutingPlan = serde_json::from_str(json).unwrap();
        assert!(plan.venue_ws_headers.is_empty());
        let round_trip: RoutingPlan =
            serde_json::from_str(&serde_json::to_string(&plan).unwrap()).unwrap();
        assert_eq!(round_trip.http.mode, HttpRouteMode::Direct);
        assert!(round_trip.public_http.is_none());
    }

    #[test]
    fn any_venue_message_resets_silence_and_channel_counters_are_independent() {
        let mut plan = RoutingPlan::default();
        plan.ws.private.fallback = Some("wss://fallback.invalid/private".into());
        plan.ws.public.fallback = Some("wss://fallback.invalid/public".into());
        let state = state(plan);
        let url = "wss://venue.invalid/shared?requestId=one";
        let private = Some(safe_cex_engine::SocketChannel::Private);
        let public = Some(safe_cex_engine::SocketChannel::Public);
        let first = state.socket(url, vec![], private.clone(), None);
        state.message(&first.key, "venue-message");
        assert_eq!(state.socket(url, vec![], private.clone(), None).url, url);
        assert_eq!(state.socket(url, vec![], private.clone(), None).url, url);
        assert_eq!(
            state.socket(url, vec![], private.clone(), None).url,
            "wss://fallback.invalid/private"
        );
        assert_eq!(state.socket(url, vec![], public, None).url, url);
        assert_eq!(
            state.socket(url, vec![], private.clone(), None).url,
            "wss://fallback.invalid/private"
        );
    }

    #[test]
    fn venue_upgrade_headers_apply_to_direct_paths_without_waf_headers() {
        let mut plan = RoutingPlan::default();
        plan.venue_ws_headers
            .insert("x-site-id".into(), "BRA_BTL".into());
        plan.ws_headers
            .insert("Origin".into(), "https://preview.tealstreet.io".into());
        let selected = state(plan).socket(
            "wss://venue.invalid/public",
            vec![("X-Api-Key".into(), "keep".into())],
            None,
            None,
        );
        assert_eq!(
            selected.headers,
            vec![
                ("X-Api-Key".into(), "keep".into()),
                ("x-site-id".into(), "BRA_BTL".into())
            ]
        );
    }

    #[test]
    fn binary_envelopes_require_valid_lengths_protocol_and_encoding() {
        for bytes in [
            b"TSB1".to_vec(),
            b"NOPE\0\0\0\x01xx".to_vec(),
            b"TSB1\0\0\0\0x".to_vec(),
            b"TSB1\0\x10\0\x01x".to_vec(),
            b"TSB1\0\0\0\x02{}".to_vec(),
        ] {
            assert!(decode_binary(&bytes).is_err());
        }
        for header in [
            r#"{"type":"rpc.auth"}"#,
            r#"{"type":"rpc.http","nonce":"one","data":{"data_encoding":"plain"}}"#,
        ] {
            let mut bytes = b"TSB1".to_vec();
            bytes.extend_from_slice(&(header.len() as u32).to_be_bytes());
            bytes.extend_from_slice(header.as_bytes());
            bytes.push(0);
            assert!(decode_binary(&bytes).is_err());
        }
    }

    #[test]
    fn body_bytes_are_preserved_instead_of_parsed_before_tunneling() {
        for body in [
            r#"{ "id" : 18446744073709551615, "n": 1.000 }"#,
            "key=one&key=two+three&signature=a%2Fb",
            "",
        ] {
            let raw = argument_body(Some(body)).unwrap();
            assert_eq!(serde_json::from_str::<String>(raw.get()).unwrap(), body);
        }
        assert_eq!(argument_body(None).unwrap().get(), "{}");
    }

    #[test]
    fn native_selectors_consume_the_exact_ts_generated_golden_matrix() {
        #[derive(Deserialize)]
        struct Example<'a> {
            name: String,
            plan: RoutingPlan,
            http: Vec<HttpExample>,
            #[serde(borrow)]
            sockets: Vec<SocketExample<'a>>,
        }
        #[derive(Deserialize)]
        struct Request {
            url: String,
            headers: BTreeMap<String, String>,
        }
        #[derive(Deserialize)]
        struct HttpExample {
            request: Request,
            expected: ExpectedHttp,
        }
        #[derive(Deserialize)]
        struct ExpectedHttp {
            url: String,
            #[serde(flatten)]
            route: HttpRoute,
        }
        #[derive(Deserialize)]
        struct SocketExample<'a> {
            channel: SocketChannel,
            original: String,
            headers: BTreeMap<String, String>,
            #[serde(default, borrow)]
            context: Option<Context<'a>>,
            #[serde(borrow)]
            attempts: Vec<ExpectedSocket<'a>>,
        }
        #[derive(Deserialize)]
        struct Context<'a> {
            feed: &'a str,
            #[serde(default)]
            recover: bool,
        }
        #[derive(Deserialize)]
        struct ExpectedSocket<'a> {
            url: String,
            relay: Option<String>,
            headers: BTreeMap<String, String>,
            #[serde(default, borrow)]
            context: Option<Context<'a>>,
            #[serde(default)]
            message: Option<String>,
        }

        let examples: Vec<Example<'static>> =
            serde_json::from_str(include_str!("../tests/fixtures/routing-plans.json")).unwrap();
        assert_eq!(examples.len(), 76);
        for example in examples {
            let state = state(example.plan);
            for http in example.http {
                let mut request = HttpRequest {
                    id: RequestId(1),
                    method: Method::Get,
                    route: None,
                    timeout_ms: None,
                    url: http.request.url,
                    headers: vec![("Content-Type", "application/json".into())],
                    signer_headers: vec![],
                    body: None,
                };
                if let Some(key) = http.request.headers.get("x-mbx-apikey") {
                    request.headers.push(("x-mbx-apikey", key.clone()));
                }
                if let Some(session) = http.request.headers.get("Authorization") {
                    request.headers.push(("Authorization", session.clone()));
                }
                let actual = state.http(&request).unwrap();
                let rewritten = if actual.prefix.is_empty() {
                    request.url.clone()
                } else {
                    format!("{}/{}", actual.prefix, request.url)
                };
                assert_eq!(rewritten, http.expected.url, "{} HTTP URL", example.name);
                assert_eq!(
                    actual.mode, http.expected.route.mode,
                    "{} HTTP mode",
                    example.name
                );
                assert_eq!(
                    actual.prefix, http.expected.route.prefix,
                    "{} HTTP prefix",
                    example.name
                );
                assert_eq!(
                    actual.authorize, http.expected.route.authorize,
                    "{} proxy auth",
                    example.name
                );
                assert_eq!(
                    actual.authorization_header, http.expected.route.authorization_header,
                    "{} auth header",
                    example.name
                );
                assert_eq!(
                    actual.headers, http.expected.route.headers,
                    "{} HTTP headers",
                    example.name
                );
                assert_eq!(
                    actual.header_aliases, http.expected.route.header_aliases,
                    "{} HTTP header aliases",
                    example.name
                );
                assert_eq!(
                    actual.tunnel_url, http.expected.route.tunnel_url,
                    "{} HOW egress",
                    example.name
                );
            }
            for socket in example.sockets {
                let role = match socket.channel {
                    SocketChannel::Public => safe_cex_engine::SocketChannel::Public,
                    SocketChannel::Private => safe_cex_engine::SocketChannel::Private,
                    SocketChannel::Business => safe_cex_engine::SocketChannel::Business,
                };
                for (attempt, expected) in socket.attempts.into_iter().enumerate() {
                    // Repeated silent attempts exercise the real selector's
                    // primary -> relay -> fallback transitions and header merge.
                    let actual = state.socket(
                        &socket.original,
                        socket.headers.clone().into_iter().collect(),
                        Some(role.clone()),
                        expected
                            .context
                            .as_ref()
                            .or(socket.context.as_ref())
                            .map(|context| safe_cex_engine::SocketContext {
                                feed: context.feed.into(),
                                recover: context.recover,
                                reject_route: false,
                            }),
                    );
                    assert_eq!(
                        actual.url, expected.url,
                        "{} {role:?} attempt {attempt}",
                        example.name
                    );
                    assert_eq!(
                        actual.relay, expected.relay,
                        "{} relay upstream",
                        example.name
                    );
                    assert_eq!(
                        actual.headers.into_iter().collect::<BTreeMap<_, _>>(),
                        expected.headers,
                        "{} WS upgrade headers",
                        example.name
                    );
                    if let Some(message) = expected.message {
                        state.message(&actual.key, &message);
                    }
                }
            }
        }
    }

    #[test]
    fn header_aliases_are_optional_case_insensitive_and_reject_conflicts() {
        let old: HttpRoute = serde_json::from_str(r#"{"mode":"direct","prefix":"","authorize":false,"authorizationHeader":"Authorization","headers":{}}"#).unwrap();
        assert!(old.header_aliases.is_empty());
        assert!(
            !serde_json::to_string(&old)
                .unwrap()
                .contains("headerAliases")
        );
        let route = HttpRoute {
            header_aliases: BTreeMap::from([(
                "Authorization".into(),
                "Coinbase-Authorization".into(),
            )]),
            ..HttpRoute::default()
        };
        let mut headers = vec![("aUtHoRiZaTiOn".into(), "Bearer venue-jwt".into())];
        route.alias_headers(&mut headers).unwrap();
        assert_eq!(
            headers,
            vec![("Coinbase-Authorization".into(), "Bearer venue-jwt".into())]
        );
        let mut missing = vec![];
        route.alias_headers(&mut missing).unwrap();
        assert!(missing.is_empty());
        let mut conflicting = vec![
            ("Authorization".into(), "Bearer venue-jwt".into()),
            ("coinbase-authorization".into(), "Bearer conflict".into()),
        ];
        let error = route.alias_headers(&mut conflicting).unwrap_err();
        assert_eq!(
            error,
            "Engine HTTP header alias conflicts with an existing header"
        );
        assert!(!error.contains("Bearer"));
    }

    fn private_route() -> WsRoute {
        WsRoute {
            host: Some("wss://proxy.invalid/bitget/v2/private".into()),
            relay: Some("wss://relay.invalid/ws?ts-relay=bitget".into()),
            fallback: Some("wss://ws.bitget.com/v2/ws/private".into()),
            ..WsRoute::default()
        }
    }

    fn connect(state: &RoutingState, url: &str, recover: bool, reject: bool) -> SelectedSocket {
        state.socket(
            url,
            vec![],
            Some(safe_cex_engine::SocketChannel::Private),
            Some(safe_cex_engine::SocketContext {
                feed: "private".into(),
                recover,
                reject_route: reject,
            }),
        )
    }

    #[test]
    fn reject_route_walks_each_route_once_then_exhausts_without_dialling() {
        let state = state(RoutingPlan {
            ws: WsRoutes {
                private: private_route(),
                ..WsRoutes::default()
            },
            ..RoutingPlan::default()
        });
        let url = "wss://ws.bitget.com/v2/ws/private";
        let first = connect(&state, url, false, false);
        assert_eq!(first.url, "wss://proxy.invalid/bitget/v2/private");
        let relay = connect(&state, url, false, true);
        assert!(!relay.exhausted);
        assert!(relay.url.starts_with("wss://relay.invalid/ws"));
        let fallback = connect(&state, url, false, true);
        assert_eq!(fallback.url, "wss://ws.bitget.com/v2/ws/private");
        let exhausted = connect(&state, url, false, true);
        assert!(exhausted.exhausted, "no route left: report, never dial");
        // The connect after exhaustion carries no reject and stays on the last route.
        let after = connect(&state, url, false, false);
        assert!(!after.exhausted);
        assert_eq!(after.url, "wss://ws.bitget.com/v2/ws/private");
    }

    #[test]
    fn reject_route_with_recover_advances_one_route_not_to_the_last() {
        let state = state(RoutingPlan {
            ws: WsRoutes {
                private: private_route(),
                ..WsRoutes::default()
            },
            ..RoutingPlan::default()
        });
        let url = "wss://ws.bitget.com/v2/ws/private";
        connect(&state, url, false, false);
        let next = connect(&state, url, true, true);
        assert!(
            next.url.starts_with("wss://relay.invalid/ws"),
            "one route forward, not recover's jump to the fallback: {}",
            next.url
        );
    }

    #[test]
    fn a_private_url_scoped_route_replaces_the_channel_route_and_a_direct_url_exhausts() {
        let state = state(RoutingPlan {
            ws: WsRoutes {
                private: WsRoute {
                    direct_urls: vec!["wss://ws.bitget.com/v3/ws/private".into()],
                    ..private_route()
                },
                ..WsRoutes::default()
            },
            private_ws_routes: vec![PublicWsRoute {
                contains: "/v2/ws/private".into(),
                route: WsRoute {
                    host: Some("wss://proxy.invalid/bitget/v2-scoped".into()),
                    ..WsRoute::default()
                },
            }],
            ..RoutingPlan::default()
        });
        let scoped = connect(&state, "wss://ws.bitget.com/v2/ws/private", false, false);
        assert_eq!(scoped.url, "wss://proxy.invalid/bitget/v2-scoped");
        let direct = connect(&state, "wss://ws.bitget.com/v3/ws/private", false, true);
        assert!(
            direct.exhausted,
            "a direct-only route has no egress to switch to"
        );
    }
}
