//! Local HTTP + WebSocket servers and URL-rewriting wrappers around the
//! production native host: real reqwest/tungstenite traffic on 127.0.0.1.
#![allow(dead_code)]
use futures_util::{SinkExt, StreamExt};
use safe_cex_exchange::{
    native::NativeRuntime,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask, RemoteSigner},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_registry::{
    native_host::{native_venue_host_signed, native_venue_host_with},
    rust_venues::rust_constructor,
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, TransportProvider, native::ReqwestAdapter,
    socket::TunnelHost,
};
use safe_cex_types::ExchangeName;
use serde_json::{Value, json};
use std::{
    cell::RefCell, collections::HashMap, future::Future, pin::Pin, rc::Rc, sync::Arc,
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::TcpListener,
    sync::mpsc,
};
use tokio_tungstenite::tungstenite::Message;

pub type Handler = Rc<dyn Fn(&Seen) -> Option<Value>>;
pub fn boundary_json(value: safe_cex_registry::native_value::NativeValue) -> Value {
    safe_cex_exchange::js_runtime::value::to_json(&value.to_js()).unwrap()
}
pub const SECRET: &str = "native-host-secret";

#[derive(Debug, Clone)]
pub struct Seen {
    pub method: String,
    pub path: String,
    pub query: String,
    pub body: String,
    pub headers: HashMap<String, String>,
}

pub fn routes() -> HashMap<String, Value> {
    let corpus: Value = serde_json::from_str(include_str!(
        "../../../safe-cex-venue-binance/tests/fixtures/public-rest.json"
    ))
    .unwrap();
    let case = |family: &str| {
        corpus
            .as_array()
            .unwrap()
            .iter()
            .find(|c| c["family"] == family && c["name"] == "normal")
            .unwrap()["payloads"]
            .clone()
    };
    let (markets, tickers) = (case("markets"), case("tickers"));
    let position = json!({"symbol":"BTCUSDT","positionSide":"BOTH","positionAmt":"0","notional":"0","entryPrice":"0","markPrice":"10","unRealizedProfit":"0","liquidationPrice":"0","leverage":"5","isolatedMargin":"0","marginType":"cross","updateTime":1});
    [
        ("GET /fapi/v1/exchangeInfo", markets[0]["data"].clone()),
        ("GET /fapi/v1/leverageBracket", json!([{"symbol":"BTCUSDT","brackets":[{"initialLeverage":125}]}])),
        ("GET /fapi/v1/ticker/24hr", tickers[0]["data"].clone()),
        ("GET /fapi/v1/ticker/bookTicker", tickers[1]["data"].clone()),
        ("GET /fapi/v1/premiumIndex", tickers[2]["data"].clone()),
        ("GET /fapi/v3/account", json!({"totalWalletBalance":"100","availableBalance":"60","totalUnrealizedProfit":"1","assets":[],"positions":[]})),
        ("GET /fapi/v3/balance", json!([{"accountAlias":"alias-native","asset":"USDT","balance":"100"}])),
        ("GET /fapi/v3/positionRisk", json!([position])),
        ("GET /fapi/v1/symbolConfig", json!([{"symbol":"BTCUSDT","marginType":"CROSSED","leverage":5}])),
        ("GET /fapi/v1/positionSide/dual", json!({"dualSidePosition":false})),
        ("GET /fapi/v1/openOrders", json!([])),
        ("GET /fapi/v1/openAlgoOrders", json!([])),
        ("POST /fapi/v1/listenKey", json!({"listenKey":"lk-native"})),
        ("PUT /fapi/v1/listenKey", json!({})),
        ("POST /fapi/v1/order", json!({"orderId":9001,"clientOrderId":"native-9001"})),
        ("DELETE /fapi/v1/order", json!({"orderId":555,"status":"CANCELED"})),
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// Minimal HTTP/1.1: one request per connection, recorded, answered from `routes`.
pub async fn serve_http(listener: TcpListener, handler: Handler, seen: Rc<RefCell<Vec<Seen>>>) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let (handler, seen) = (handler.clone(), seen.clone());
        tokio::task::spawn_local(async move {
            let mut buf = vec![];
            let mut chunk = [0u8; 4096];
            let head_end = loop {
                let n = stream.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    return;
                }
                buf.extend_from_slice(&chunk[..n]);
                if let Some(i) = buf.windows(4).position(|w| w == b"\r\n\r\n") {
                    break i;
                }
            };
            let head = String::from_utf8_lossy(&buf[..head_end]).into_owned();
            let mut lines = head.split("\r\n");
            let mut start = lines.next().unwrap().split(' ');
            let (method, target) = (
                start.next().unwrap().to_owned(),
                start.next().unwrap().to_owned(),
            );
            let headers: HashMap<String, String> = lines
                .filter_map(|l| l.split_once(':'))
                .map(|(k, v)| (k.trim().to_ascii_lowercase(), v.trim().to_owned()))
                .collect();
            let length: usize = headers
                .get("content-length")
                .and_then(|v| v.parse().ok())
                .unwrap_or(0);
            while buf.len() < head_end + 4 + length {
                let n = stream.read(&mut chunk).await.unwrap_or(0);
                if n == 0 {
                    break;
                }
                buf.extend_from_slice(&chunk[..n]);
            }
            let (path, query) = target.split_once('?').unwrap_or((&target, ""));
            let request = Seen {
                method: method.clone(),
                path: path.into(),
                query: query.into(),
                body: String::from_utf8_lossy(&buf[head_end + 4..head_end + 4 + length])
                    .into_owned(),
                headers,
            };
            let answer = handler(&request);
            seen.borrow_mut().push(request);
            let (status, body) = match answer {
                // A handler picks a non-200 status with {"__status": 400, "body": ...}.
                Some(reply) if reply.get("__status").is_some() => (
                    match reply["__status"].as_u64() {
                        Some(200) => "200 OK",
                        Some(400) => "400 Bad Request",
                        Some(401) => "401 Unauthorized",
                        Some(403) => "403 Forbidden",
                        Some(418) => "418 I'm a teapot",
                        Some(429) => "429 Too Many Requests",
                        Some(500) => "500 Internal Server Error",
                        Some(502) => "502 Bad Gateway",
                        other => panic!("unsupported fixture status {other:?}"),
                    },
                    reply["body"].to_string(),
                ),
                Some(body) => ("200 OK", body.to_string()),
                None => (
                    "404 Not Found",
                    json!({"code":-1,"msg":format!("no route {method} {path}")}).to_string(),
                ),
            };
            let response = format!(
                "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
                body.len()
            );
            let _ = stream.write_all(response.as_bytes()).await;
            let _ = stream.shutdown().await;
        });
    }
}

pub struct WsPeer {
    pub path: String,
    pub push: mpsc::UnboundedSender<String>,
}
pub async fn serve_ws(
    listener: TcpListener,
    peers: Rc<RefCell<Vec<WsPeer>>>,
    received: Rc<RefCell<Vec<String>>>,
) {
    loop {
        let Ok((stream, _)) = listener.accept().await else {
            return;
        };
        let (peers, received) = (peers.clone(), received.clone());
        tokio::task::spawn_local(async move {
            let path = Rc::new(RefCell::new(String::new()));
            let captured = path.clone();
            let callback =
                move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                      response| {
                    *captured.borrow_mut() = request.uri().to_string();
                    Ok(response)
                };
            let Ok(socket) = tokio_tungstenite::accept_hdr_async(stream, callback).await else {
                return;
            };
            let (mut sink, mut source) = socket.split();
            let (push, mut outgoing) = mpsc::unbounded_channel::<String>();
            peers.borrow_mut().push(WsPeer {
                path: path.borrow().clone(),
                push,
            });
            tokio::task::spawn_local(async move {
                while let Some(text) = outgoing.recv().await {
                    if sink.send(Message::text(text)).await.is_err() {
                        break;
                    }
                }
            });
            while let Some(Ok(message)) = source.next().await {
                if let Message::Text(text) = message {
                    received.borrow_mut().push(text.to_string());
                }
            }
        });
    }
}

/// Production reqwest adapter; only the venue origin is pointed at the local server.
pub struct LocalOrigin {
    pub inner: ReqwestAdapter,
    pub origin: String,
}
pub fn rewrite(url: &str, origin: &str) -> String {
    match url
        .split_once("://")
        .and_then(|(_, rest)| rest.split_once('/').map(|(_, path)| path))
    {
        Some(path) => format!("{origin}/{path}"),
        None if url.contains("://") => origin.to_owned(),
        None => url.to_owned(),
    }
}
impl RestAdapter for LocalOrigin {
    fn execute(&self, mut request: RestRequest) -> RestFuture<'_, RestResponse> {
        request.base_url = request.base_url.map(|b| rewrite(&b, &self.origin));
        if request.url.starts_with("http") {
            request.url = rewrite(&request.url, &self.origin);
        }
        self.inner.execute(request)
    }
}

/// Production NativeRuntime; only WebSocket dials are pointed at the local server.
pub struct LocalDial {
    pub inner: Rc<NativeRuntime>,
    pub origin: String,
}
impl ExchangeRuntime for LocalDial {
    fn monotonic_now_ms(&self) -> f64 {
        self.inner.monotonic_now_ms()
    }
    fn now_ms(&self) -> f64 {
        self.inner.now_ms()
    }
    fn sleep(&self, delay_ms: u64) -> ExchangeFuture<'static, ()> {
        self.inner.sleep(delay_ms)
    }
    fn spawn_local(&self, future: Pin<Box<dyn Future<Output = ()>>>) -> Box<dyn ExchangeTask> {
        self.inner.spawn_local(future)
    }
    fn console_values(&self, severity: &str, message: &[safe_cex_exchange::js_runtime::value::Js]) {
        self.inner.console_values(severity, message)
    }
    fn unique_id(&self) -> String {
        self.inner.unique_id()
    }
    fn is_worker(&self) -> bool {
        false
    }
    fn random(&self) -> f64 {
        self.inner.random()
    }
    fn report_exception(&self, error: &ExchangeError, context: &Value) {
        self.inner.report_exception(error, context)
    }
    fn report_raw_exception(
        &self,
        error: &safe_cex_exchange::js_runtime::value::Js,
        context: &Value,
    ) {
        self.inner.report_raw_exception(error, context)
    }
    fn execute_worker_request(&self, request: Value) -> ExchangeFuture<'static, Value> {
        self.inner.execute_worker_request(request)
    }
}
impl WsRuntime for LocalDial {
    fn transport(&self) -> Arc<dyn TransportProvider> {
        self.inner.transport()
    }
    fn tunnel_host(&self) -> Rc<dyn TunnelHost> {
        self.inner.tunnel_host()
    }
    fn dial(
        &self,
        url: String,
        protocols: Vec<String>,
        options: WebSocketConnectionOptions,
        reconnecting: bool,
    ) -> ExchangeFuture<'static, Rc<dyn WsSocket>> {
        self.inner.dial(
            rewrite(&url, &self.origin),
            protocols,
            options,
            reconnecting,
        )
    }
    fn headers_capable(&self) -> bool {
        self.inner.headers_capable()
    }
    fn enable_reconnecting(&self) -> bool {
        self.inner.enable_reconnecting()
    }
    fn min_reconnection_delay_ms(&self) -> f64 {
        self.inner.min_reconnection_delay_ms()
    }
    fn library_min_reconnection_delay_ms(&self) -> f64 {
        self.inner.library_min_reconnection_delay_ms()
    }
    fn diagnostics_enabled(&self) -> bool {
        false
    }
    fn lifecycle_logs_enabled(&self) -> bool {
        false
    }
}

pub async fn eventually(what: &str, mut ready: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
pub fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
}

/// Answers from `routes()` unless `extra` answers first.
pub fn handler(extra: Handler) -> Handler {
    let table = routes();
    Rc::new(move |request| {
        extra(request).or_else(|| {
            table
                .get(&format!("{} {}", request.method, request.path))
                .cloned()
        })
    })
}
pub struct Harness {
    pub exchange: Rc<dyn safe_cex_exchange::exchange::Exchange>,
    pub store: Rc<dyn Store>,
    pub seen: Rc<RefCell<Vec<Seen>>>,
    pub peers: Rc<RefCell<Vec<WsPeer>>>,
    /// Text frames every local socket received from the exchange.
    pub received: Rc<RefCell<Vec<String>>>,
}
/// Binance built through the registry's Rust constructor on the native host. Call inside a LocalSet.
pub async fn binance_harness(extra: Handler) -> Harness {
    binance_harness_with(extra, json!({"key":"native-key","secret":SECRET})).await
}
pub async fn binance_harness_with(extra: Handler, options: Value) -> Harness {
    binance_harness_on(extra, options, None).await
}
/// As `binance_harness_with`, signing through a hosted remote signer when one is given.
pub async fn binance_harness_on(
    extra: Handler,
    options: Value,
    signer: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
) -> Harness {
    binance_harness_over(
        extra,
        options,
        signer,
        safe_cex_exchange::native::native_runtime(),
    )
    .await
}
/// As `binance_harness_on`, over a caller-built native runtime (e.g. the Tealstreet transport).
pub async fn binance_harness_over(
    extra: Handler,
    options: Value,
    signer: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
    runtime: Rc<NativeRuntime>,
) -> Harness {
    venue_harness(ExchangeName::Binance, extra, options, signer, runtime).await
}
/// Any Rust-constructed venue, built through the registry on the native host.
pub async fn venue_harness(
    name: ExchangeName,
    extra: Handler,
    options: Value,
    signer: Option<(Rc<dyn RemoteSigner>, Option<String>)>,
    runtime: Rc<NativeRuntime>,
) -> Harness {
    let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let ws = TcpListener::bind("127.0.0.1:0").await.unwrap();
    let http_origin = format!("http://{}", http.local_addr().unwrap());
    let ws_origin = format!("ws://{}", ws.local_addr().unwrap());
    let seen = Rc::new(RefCell::new(vec![]));
    let peers = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_http(http, handler(extra), seen.clone()));
    let received = Rc::new(RefCell::new(vec![]));
    tokio::task::spawn_local(serve_ws(ws, peers.clone(), received.clone()));
    let host = Rc::new(LocalDial {
        inner: runtime,
        origin: ws_origin,
    });
    let adapters = Rc::new(move || {
        let inner = safe_cex_transport::native::reqwest_adapter()
            .map_err(|e| ExchangeError::new(e.to_string()))?;
        Ok(Arc::new(LocalOrigin {
            inner,
            origin: http_origin.clone(),
        }) as Arc<dyn RestAdapter>)
    });
    let host = match signer {
        Some((signer, account)) => native_venue_host_signed(
            host,
            adapters,
            signer,
            account,
            safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        ),
        None => native_venue_host_with(
            host,
            adapters,
            safe_cex_registry::native_host::hyperliquid_rate_limiter(),
        ),
    };
    let constructor =
        rust_constructor(&name, host).expect("a Rust-constructed venue in the registry table");
    let options = serde_json::from_value(options).unwrap();
    let store: Rc<dyn Store> = Rc::new(safe_cex_store::store::DefaultStore::new());
    let exchange = constructor
        .construct(name, options, store.clone())
        .unwrap_or_else(|e| {
            panic!(
                "registry constructs the venue on the native host: {}",
                e.message
            )
        });
    Harness {
        exchange,
        store,
        seen,
        peers,
        received,
    }
}
pub fn run_local(future: impl std::future::Future<Output = ()>) {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, future);
}
