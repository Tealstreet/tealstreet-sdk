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
/// Axios buildURL's default params encoding for params no factory codec consumed.
fn axios_default_query(params: &serde_json::Value) -> String {
    fn encode(text: &str) -> String {
        url::form_urlencoded::byte_serialize(text.as_bytes())
            .collect::<String>()
            .replace("%3A", ":")
            .replace("%24", "$")
            .replace("%2C", ",")
            .replace("%5B", "[")
            .replace("%5D", "]")
    }
    fn scalar(value: &serde_json::Value) -> Option<String> {
        match value {
            serde_json::Value::Null => None,
            serde_json::Value::String(s) => Some(s.clone()),
            serde_json::Value::Number(n) => n
                .as_f64()
                .map(safe_cex_num::js_number_string)
                .or_else(|| Some(n.to_string())),
            other => Some(other.to_string()),
        }
    }
    let serde_json::Value::Object(map) = params else {
        return String::new();
    };
    let mut parts = vec![];
    for (key, value) in map {
        match value {
            serde_json::Value::Array(items) => {
                for item in items.iter().filter_map(scalar) {
                    parts.push(format!("{}[]={}", encode(key), encode(&item)));
                }
            }
            other => {
                if let Some(text) = scalar(other) {
                    parts.push(format!("{}={}", encode(key), encode(&text)));
                }
            }
        }
    }
    parts.join("&")
}
fn network_failure(error: &reqwest::Error, request: &RestRequest) -> RestFailure {
    let (code, message) = if error.is_timeout() {
        let ms = request.timeout_ms.unwrap_or(0);
        ("ECONNABORTED", format!("timeout of {ms}ms exceeded"))
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
                let query = axios_default_query(params);
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
pub struct NativeSocket {
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
    let connector = tokio_tungstenite::Connector::Rustls(tls_config()?);
    let (stream, _) =
        tokio_tungstenite::connect_async_tls_with_config(request, None, false, Some(connector))
            .await
            .map_err(|e| {
                TransportError::Network(NetworkFailure::from_code(
                    format!("WebSocket {url}: {e}"),
                    Some("ECONNREFUSED".into()),
                ))
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
    let reader_state = state.clone();
    let reader = tokio::task::spawn_local(async move {
        let mut close = SocketFrame::Close {
            code: 1006,
            reason: String::new(),
            was_clean: false,
        };
        while let Some(message) = source.next().await {
            let frame = match message {
                Ok(Message::Text(text)) => SocketFrame::Text(text.to_string()),
                Ok(Message::Binary(bytes)) => SocketFrame::Binary(bytes.to_vec()),
                Ok(Message::Close(frame)) => {
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
        state,
        outgoing,
        incoming: Rc::new(Mutex::new(incoming)),
        reader,
    })
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
            alpn: false,
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
