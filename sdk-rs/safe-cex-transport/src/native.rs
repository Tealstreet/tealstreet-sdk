//! Native terminal I/O: the physical HTTP adapter and WebSocket under every
//! factory, signer and codec. Must run inside a tokio `LocalSet`.
use crate::{
    NetworkFailure, RestAdapter, RestBody, RestFailure, RestFuture, RestRequest, RestResponse,
    RestResponseBody, TransportError, TransportFuture,
    socket::{
        SocketCapabilities, SocketFrame, SocketRuntime, TunnelDialOptions, TunnelHost,
        TunnelSocket, TunnelTask,
    },
};
use futures_util::{SinkExt, StreamExt};
use std::{
    cell::Cell,
    collections::BTreeMap,
    future::Future,
    pin::Pin,
    rc::Rc,
    sync::{Arc, OnceLock},
    time::{Duration, SystemTime, UNIX_EPOCH},
};
use tokio::sync::{Mutex, mpsc};
use tokio_tungstenite::tungstenite::{
    Message,
    client::IntoClientRequest,
    protocol::{CloseFrame, frame::coding::CloseCode},
};

/// One rustls configuration on the ring provider with the platform roots.
fn tls_config() -> Result<Arc<rustls::ClientConfig>, TransportError> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, String>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut roots = rustls::RootCertStore::empty();
            let loaded = rustls_native_certs::load_native_certs();
            for cert in loaded.certs {
                roots
                    .add(cert)
                    .map_err(|e| format!("native root rejected: {e}"))?;
            }
            if roots.is_empty() {
                return Err(format!("no native TLS roots loaded: {:?}", loaded.errors));
            }
            let provider = Arc::new(rustls::crypto::ring::default_provider());
            rustls::ClientConfig::builder_with_provider(provider)
                .with_safe_default_protocol_versions()
                .map(|b| Arc::new(b.with_root_certificates(roots).with_no_client_auth()))
                .map_err(|e| format!("TLS configuration: {e}"))
        })
        .clone()
        .map_err(TransportError::Host)
}

/// WebSocket upgrades are HTTP/1.1, so every socket dial offers exactly that ALPN
/// (ts: the vendored ReconnectingWebSocket's ALPNProtocols, which tradetunnel requires).
fn ws_tls_config() -> Result<Arc<rustls::ClientConfig>, TransportError> {
    static CONFIG: OnceLock<Result<Arc<rustls::ClientConfig>, TransportError>> = OnceLock::new();
    CONFIG
        .get_or_init(|| {
            let mut config = (*tls_config()?).clone();
            config.alpn_protocols = vec![b"http/1.1".to_vec()];
            Ok(Arc::new(config))
        })
        .clone()
}

/// Axios-shaped terminal adapter: sends exactly the bytes it is given and
/// settles non-2xx statuses as response-bearing failures, like `settle()`.
pub struct ReqwestAdapter {
    client: reqwest::Client,
}
pub fn reqwest_adapter() -> Result<ReqwestAdapter, TransportError> {
    let tls = (*tls_config()?).clone();
    reqwest::Client::builder()
        .tls_backend_preconfigured(tls)
        .build()
        .map(|client| ReqwestAdapter { client })
        .map_err(|e| TransportError::Host(format!("HTTP client: {e}")))
}
fn request_url(request: &RestRequest) -> String {
    // Axios buildFullPath/isAbsoluteURL and combineURLs: join at one slash,
    // preserve absolute (including protocol-relative) URLs and an empty path.
    let absolute = request.url.starts_with("//")
        || request.url.split_once("://").is_some_and(|(scheme, _)| {
            let mut bytes = scheme.bytes();
            bytes.next().is_some_and(|c| c.is_ascii_alphabetic())
                && bytes.all(|c| c.is_ascii_alphanumeric() || matches!(c, b'+' | b'-' | b'.'))
        });
    let Some(base) = request.base_url.as_deref().filter(|base| !base.is_empty()) else {
        return request.url.clone();
    };
    if absolute {
        request.url.clone()
    } else if request.url.is_empty() {
        base.to_owned()
    } else {
        format!(
            "{}/{}",
            base.trim_end_matches('/'),
            request.url.trim_start_matches('/')
        )
    }
}

/// Axios default traversal is shared with the native-number JS host frontdoor.
fn axios_default_query(params: &serde_json::Value) -> Result<String, TransportError> {
    crate::query::axios_default_query(&params.clone().into())
}
fn network_failure(error: &reqwest::Error, request: &RestRequest) -> RestFailure {
    if error.is_builder()
        && let Ok(url) = reqwest::Url::parse(&request_url(request))
        && !matches!(url.scheme(), "http" | "https" | "file" | "data")
    {
        let mut failure = RestFailure::pre_transmit(
            format!("Unsupported protocol {}:", url.scheme()),
            request.clone(),
        );
        failure.code = Some("ERR_BAD_REQUEST".into());
        return failure;
    }
    let mut cause = std::error::Error::source(error);
    let mut originating_code = None;
    let mut detail = error.to_string();
    while let Some(current) = cause {
        detail = current.to_string();
        // Hyper's connector erases its DNS error type but retains this context.
        if detail == "dns error" {
            originating_code = Some("ENOTFOUND");
        }
        if originating_code == Some("ENOTFOUND")
            && detail.contains("Temporary failure in name resolution")
        {
            originating_code = Some("EAI_AGAIN");
        }
        if let Some(rustls::Error::InvalidCertificate(certificate)) =
            current.downcast_ref::<rustls::Error>()
        {
            use rustls::CertificateError;
            originating_code = Some(match certificate {
                CertificateError::Expired | CertificateError::ExpiredContext { .. } => {
                    "CERT_HAS_EXPIRED"
                }
                CertificateError::NotValidYet | CertificateError::NotValidYetContext { .. } => {
                    "CERT_NOT_YET_VALID"
                }
                CertificateError::Revoked => "CERT_REVOKED",
                CertificateError::NotValidForName
                | CertificateError::NotValidForNameContext { .. } => "ERR_TLS_CERT_ALTNAME_INVALID",
                CertificateError::BadSignature => "CERT_SIGNATURE_FAILURE",
                CertificateError::InvalidPurpose
                | CertificateError::InvalidPurposeContext { .. } => "INVALID_PURPOSE",
                _ => "CERT_REJECTED",
            });
        }
        if let Some(io) = current.downcast_ref::<std::io::Error>()
            && io.kind() == std::io::ErrorKind::NetworkUnreachable
        {
            originating_code = Some("ENETUNREACH");
        }
        // io::Error::source skips its immediate custom error; TLS lives there.
        cause = current
            .downcast_ref::<std::io::Error>()
            .and_then(|io| io.get_ref().map(|inner| inner as &dyn std::error::Error))
            .or_else(|| current.source());
    }
    let (code, message) = if error.is_timeout() {
        let ms = request.timeout_ms.unwrap_or(0);
        ("ECONNABORTED", format!("timeout of {ms}ms exceeded"))
    } else if let Some(code) = originating_code {
        (code, format!("{code}: {detail}"))
    } else if error.is_connect() {
        (
            "ECONNREFUSED",
            format!("connect ECONNREFUSED {}", request_url(request)),
        )
    } else {
        ("ECONNRESET", format!("socket hang up: {error}"))
    };
    RestFailure::from_transport(
        TransportError::Network(NetworkFailure::from_code(message, Some(code.into()))),
        request.clone(),
    )
}
impl RestAdapter for ReqwestAdapter {
    fn execute(&self, request: RestRequest) -> RestFuture<'_, RestResponse> {
        Box::pin(async move {
            let mut url = request_url(&request);
            if let Some(params) = request
                .params
                .as_ref()
                .filter(|p| safe_cex_num::js_truthy(Some(p)))
            {
                let query = axios_default_query(params)
                    .map_err(|error| RestFailure::from_transport(error, request.clone()))?;
                if !query.is_empty() {
                    if let Some(hash) = url.find('#') {
                        url.truncate(hash);
                    }
                    url.push(if url.contains('?') { '&' } else { '?' });
                    url.push_str(&query);
                }
            }
            let method = reqwest::Method::from_bytes(request.method.to_uppercase().as_bytes())
                .map_err(|e| {
                    RestFailure::pre_transmit(format!("HTTP method: {e}"), request.clone())
                })?;
            let mut builder = self.client.request(method, &url);
            for (name, value) in &request.headers {
                builder = builder.header(name, value);
            }
            if let Some(ms) = request.timeout_ms.filter(|ms| *ms > 0) {
                builder = builder.timeout(Duration::from_millis(ms as u64));
            }
            match &request.body {
                None => {}
                Some(RestBody::Encoded(bytes)) => builder = builder.body(bytes.clone()),
                Some(body) => {
                    let bytes = body
                        .wire_bytes()
                        .map_err(|e| RestFailure::from_transport(e, request.clone()))?;
                    builder = builder.body(bytes);
                }
            }
            let response = builder
                .send()
                .await
                .map_err(|e| network_failure(&e, &request))?;
            let status = response.status();
            let headers: BTreeMap<String, String> = response
                .headers()
                .iter()
                .filter_map(|(k, v)| {
                    v.to_str()
                        .ok()
                        .map(|v| (k.as_str().to_owned(), v.to_owned()))
                })
                .collect();
            let body = response
                .bytes()
                .await
                .map_err(|e| network_failure(&e, &request))?;
            let response = RestResponse {
                status: status.as_u16(),
                reason: status.canonical_reason().map(str::to_owned),
                headers,
                body: RestResponseBody::Encoded(body.to_vec()),
            };
            if status.is_success() {
                return Ok(response);
            }
            let code = if status.is_client_error() {
                "ERR_BAD_REQUEST"
            } else {
                "ERR_BAD_RESPONSE"
            };
            Err(RestFailure {
                code: Some(code.into()),
                transport_error: None,
                message: format!("Request failed with status code {}", status.as_u16()),
                response: Some(Box::new(response)),
                request: Box::new(request),
                context: None,
            })
        })
    }
}

/// A client WebSocket. Events are queued from connection, so none are lost
/// before the consumer's first `receive`.
#[derive(Default, Clone, serde::Serialize)]
pub struct NativeFrameCounts {
    pub message: u64,
    pub ping: u64,
    pub pong: u64,
    pub close: u64,
}
pub struct NativeSocket {
    /// Physical frames consumed by tungstenite, before application dispatch.
    pub frame_counts: Rc<std::cell::RefCell<NativeFrameCounts>>,
    state: Rc<Cell<u16>>,
    outgoing: mpsc::UnboundedSender<Message>,
    incoming: Rc<Mutex<mpsc::UnboundedReceiver<SocketFrame>>>,
    reader: tokio::task::AbortHandle,
}
impl Drop for NativeSocket {
    fn drop(&mut self) {
        self.reader.abort();
    }
}
pub async fn connect_websocket(
    url: &str,
    protocols: &[String],
    headers: &BTreeMap<String, String>,
) -> Result<NativeSocket, TransportError> {
    prepare_websocket(url, protocols, headers)?.await
}

/// Validate constructor inputs before exposing a CONNECTING host handle. The
/// returned future owns the single physical handshake and its pending I/O.
pub fn prepare_websocket(
    url: &str,
    protocols: &[String],
    headers: &BTreeMap<String, String>,
) -> Result<TransportFuture<'static, NativeSocket>, TransportError> {
    let host_error =
        |e: &dyn std::fmt::Display| TransportError::Host(format!("WebSocket {url}: {e}"));
    let mut request = url.into_client_request().map_err(|e| host_error(&e))?;
    for (name, value) in headers {
        let name = tokio_tungstenite::tungstenite::http::HeaderName::from_bytes(name.as_bytes())
            .map_err(|e| host_error(&e))?;
        request
            .headers_mut()
            .insert(name, value.parse().map_err(|e| host_error(&e))?);
    }
    if !protocols.is_empty() {
        request.headers_mut().insert(
            "Sec-WebSocket-Protocol",
            protocols.join(", ").parse().map_err(|e| host_error(&e))?,
        );
    }
    tokio_tungstenite::tungstenite::client::uri_mode(request.uri()).map_err(|e| host_error(&e))?;
    let connector = tokio_tungstenite::Connector::Rustls(ws_tls_config()?);
    let url = url.to_owned();
    Ok(Box::pin(async move {
        let (stream, _) =
            tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
                .await
                .map_err(|e| match e {
                    tokio_tungstenite::tungstenite::Error::Url(_) => {
                        TransportError::Host(format!("WebSocket {url}: {e}"))
                    }
                    _ => TransportError::Network(NetworkFailure::from_code(
                        format!("WebSocket {url}: {e}"),
                        Some("ECONNREFUSED".into()),
                    )),
                })?;
        let (mut sink, mut source) = stream.split();
        let state = Rc::new(Cell::new(1));
        let (outgoing, mut queued) = mpsc::unbounded_channel::<Message>();
        let (deliver, incoming) = mpsc::unbounded_channel::<SocketFrame>();
        let writer_state = state.clone();
        tokio::task::spawn_local(async move {
            while let Some(message) = queued.recv().await {
                let closing = matches!(message, Message::Close(_));
                if sink.send(message).await.is_err() || closing {
                    break;
                }
            }
            writer_state.set(3);
        });
        let frame_counts = Rc::new(std::cell::RefCell::new(NativeFrameCounts::default()));
        let reader_counts = frame_counts.clone();
        let reader_state = state.clone();
        let reader = tokio::task::spawn_local(async move {
            let mut close = SocketFrame::Close {
                code: 1006,
                reason: String::new(),
                was_clean: false,
            };
            while let Some(message) = source.next().await {
                let frame = match message {
                    Ok(Message::Text(text)) => {
                        reader_counts.borrow_mut().message += 1;
                        SocketFrame::Text(text.to_string())
                    }
                    Ok(Message::Binary(bytes)) => {
                        reader_counts.borrow_mut().message += 1;
                        SocketFrame::Binary(bytes.to_vec())
                    }
                    Ok(Message::Ping(_)) => {
                        reader_counts.borrow_mut().ping += 1;
                        continue;
                    }
                    Ok(Message::Pong(_)) => {
                        reader_counts.borrow_mut().pong += 1;
                        continue;
                    }
                    Ok(Message::Close(frame)) => {
                        reader_counts.borrow_mut().close += 1;
                        close = frame.map_or(
                            SocketFrame::Close {
                                code: 1005,
                                reason: String::new(),
                                was_clean: true,
                            },
                            |f| SocketFrame::Close {
                                code: f.code.into(),
                                reason: f.reason.to_string(),
                                was_clean: true,
                            },
                        );
                        break;
                    }
                    Ok(_) => continue,
                    Err(_) => break,
                };
                if deliver.send(frame).is_err() {
                    break;
                }
            }
            reader_state.set(3);
            let _ = deliver.send(close);
        })
        .abort_handle();
        Ok(NativeSocket {
            frame_counts,
            state,
            outgoing,
            incoming: Rc::new(Mutex::new(incoming)),
            reader,
        })
    }))
}

impl TunnelSocket for NativeSocket {
    fn ready_state(&self) -> u16 {
        self.state.get()
    }
    fn send(&self, frame: SocketFrame) -> Result<(), TransportError> {
        if self.state.get() != 1 {
            return Err(TransportError::Host("WebSocket is not open".into()));
        }
        let message = match frame {
            SocketFrame::Text(text) => Message::text(text),
            SocketFrame::Binary(bytes) => Message::binary(bytes),
            SocketFrame::Close { code, reason, .. } => Message::Close(Some(CloseFrame {
                code: CloseCode::from(code),
                reason: reason.into(),
            })),
        };
        self.outgoing
            .send(message)
            .map_err(|_| TransportError::Host("WebSocket writer closed".into()))
    }
    fn receive(&self) -> TransportFuture<'static, SocketFrame> {
        let incoming = self.incoming.clone();
        Box::pin(async move {
            incoming
                .lock()
                .await
                .recv()
                .await
                .ok_or_else(|| TransportError::Host("WebSocket event stream ended".into()))
        })
    }
    fn close(&self, code: Option<u16>, reason: &str) -> Result<(), TransportError> {
        if self.state.get() >= 2 {
            return Ok(());
        }
        self.state.set(2);
        let frame = CloseFrame {
            code: CloseCode::from(code.unwrap_or(1000)),
            reason: reason.to_owned().into(),
        };
        self.outgoing
            .send(Message::Close(Some(frame)))
            .map_err(|_| TransportError::Host("WebSocket writer closed".into()))
    }
}

struct LocalTask(tokio::task::AbortHandle);
impl TunnelTask for LocalTask {
    fn cancel(&self) {
        self.0.abort();
    }
}
pub fn spawn_local_task(future: Pin<Box<dyn Future<Output = ()>>>) -> tokio::task::AbortHandle {
    tokio::task::spawn_local(future).abort_handle()
}
pub fn wall_clock_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0.0, |d| d.as_secs_f64() * 1000.0)
}

/// Tunnel host for the direct transport: plain dials only. It holds no
/// Tealstreet proxy token, so tunnel/relay routes fail visibly rather than going direct.
#[derive(Default)]
pub struct NativeTunnelHost;
impl TunnelHost for NativeTunnelHost {
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn TunnelTask> {
        Box::new(LocalTask(spawn_local_task(future)))
    }
    fn capabilities(&self) -> SocketCapabilities {
        SocketCapabilities {
            runtime: SocketRuntime::Native,
            upgrade_headers: true,
            alpn: true,
        }
    }
    fn now_ms(&self) -> u64 {
        wall_clock_ms() as u64
    }
    fn sleep_until(&self, deadline_ms: u64) -> TransportFuture<'static, ()> {
        let wait = deadline_ms.saturating_sub(self.now_ms());
        Box::pin(async move {
            tokio::time::sleep(Duration::from_millis(wait)).await;
            Ok(())
        })
    }
    fn connect(
        &self,
        url: String,
        options: TunnelDialOptions,
    ) -> TransportFuture<'static, Rc<dyn TunnelSocket>> {
        Box::pin(async move {
            let socket = connect_websocket(&url, options.protocols(), options.headers()).await?;
            Ok(Rc::new(socket) as Rc<dyn TunnelSocket>)
        })
    }
    fn proxy_token(&self) -> TransportFuture<'_, String> {
        Box::pin(async {
            Err(TransportError::UnavailableTransport(
                "the native direct host has no Tealstreet proxy token; register the Tealstreet transport for tunnel or relay routes".into(),
            ))
        })
    }
    fn is_token_expired(&self, _token: &str) -> bool {
        true
    }
    fn sync_server_time(&self, _server_time: u64) {}
}

#[cfg(test)]
#[path = "native_network_tests.rs"]
mod network_tests;

#[cfg(test)]
mod url_tests {
    use super::*;

    #[test]
    fn unsupported_protocol_is_a_nonretryable_setup_failure() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        runtime.block_on(async {
            let adapter = reqwest_adapter().unwrap();
            for url in ["unsupported://fixture", "FTP://fixture/orders"] {
                let failure = adapter
                    .execute(RestRequest {
                        method: "GET".into(),
                        url: url.into(),
                        ..Default::default()
                    })
                    .await
                    .unwrap_err();
                let scheme = reqwest::Url::parse(url).unwrap();
                assert_eq!(failure.code.as_deref(), Some("ERR_BAD_REQUEST"));
                assert_eq!(
                    failure.message,
                    format!("Unsupported protocol {}:", scheme.scheme())
                );
                assert!(!failure.is_retryable_network());
                assert!(failure.response.is_none());
            }
        });
    }

    #[test]
    fn native_url_joins_base_and_relative_paths_like_axios() {
        for (base, path, expected) in [
            (
                Some("https://dx.test/dxsca-web"),
                "accounts/A/orders",
                "https://dx.test/dxsca-web/accounts/A/orders",
            ),
            (
                Some("https://dx.test/dxsca-web///"),
                "/login",
                "https://dx.test/dxsca-web/login",
            ),
            (
                Some("https://dx.test/dxsca-web"),
                "http-price",
                "https://dx.test/dxsca-web/http-price",
            ),
            (
                Some("https://dx.test/dxsca-web/"),
                "",
                "https://dx.test/dxsca-web/",
            ),
            (
                Some("https://dx.test"),
                "HTTPS://other.test/orders",
                "HTTPS://other.test/orders",
            ),
            (
                Some("https://dx.test"),
                "//other.test/orders",
                "//other.test/orders",
            ),
            (None, "https://dx.test/orders", "https://dx.test/orders"),
            (Some(""), "/login", "/login"),
        ] {
            let request = RestRequest {
                base_url: base.map(str::to_owned),
                url: path.into(),
                ..Default::default()
            };
            assert_eq!(
                request_url(&request),
                expected,
                "AXIOS URL JOIN {base:?} {path}"
            );
        }
    }
}

#[cfg(test)]
mod query_tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn default_query_matches_installed_axios_bytes() {
        let mut failures = Vec::new();
        for (params, expected) in [
            (json!({"a":[1,2]}), "a%5B%5D=1&a%5B%5D=2"),
            (json!({"a":"f[g]h"}), "a=f%5Bg%5Dh"),
            (json!({"a":"x~y!z(1)'"}), "a=x~y!z(1)'"),
            (json!({"a":{"b":1}}), "a%5Bb%5D=1"),
            (json!({"a":[{"b":1}]}), "a%5B0%5D%5Bb%5D=1"),
            (
                json!({"tags[]":["a b",null,"x+y"]}),
                "tags%5B%5D=a+b&tags%5B%5D=x%2By",
            ),
        ] {
            let actual = axios_default_query(&params).expect("valid default params");
            if actual != expected {
                failures.push(format!(
                    "params={params}: expected {expected:?}, actual {actual:?}"
                ));
            }
        }
        assert!(
            failures.is_empty(),
            "native Axios default query mismatch: {failures:#?}"
        );
    }
}

#[cfg(test)]
mod drain_tests {
    use super::*;
    #[test]
    fn native_frame_receipts_include_control_frames_before_application_dispatch() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        tokio::task::LocalSet::new().block_on(&runtime, async {
            let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
            let url = format!("ws://{}", listener.local_addr().unwrap());
            let server = tokio::task::spawn_local(async move {
                let (stream, _) = listener.accept().await.unwrap();
                let mut peer = tokio_tungstenite::accept_async(stream).await.unwrap();
                for message in [
                    Message::text("one"),
                    Message::binary(vec![0, 255]),
                    Message::Ping(vec![1].into()),
                    Message::Pong(vec![2].into()),
                    Message::Close(None),
                ] {
                    peer.send(message).await.unwrap();
                }
            });
            let socket = connect_websocket(&url, &[], &BTreeMap::new())
                .await
                .unwrap();
            assert!(matches!(
                socket.receive().await.unwrap(),
                SocketFrame::Text(_)
            ));
            assert!(matches!(
                socket.receive().await.unwrap(),
                SocketFrame::Binary(_)
            ));
            assert!(matches!(
                socket.receive().await.unwrap(),
                SocketFrame::Close { .. }
            ));
            server.await.unwrap();
            let counts = socket.frame_counts.borrow();
            assert_eq!(
                (counts.message, counts.ping, counts.pong, counts.close),
                (2, 1, 1, 1)
            );
        });
    }
}
