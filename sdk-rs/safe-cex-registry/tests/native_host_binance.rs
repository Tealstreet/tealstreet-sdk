//! BinanceExchange constructed by the registry on the production native host, against a local
//! HTTP + WebSocket server: real reqwest and tungstenite traffic on 127.0.0.1.
//! Only the venue hostnames are rewritten; signing, codecs and sockets are production.
#![cfg(not(target_arch = "wasm32"))]
use futures_util::{SinkExt, StreamExt};
use hmac::{Hmac, Mac};
use safe_cex_exchange::{
    native::NativeRuntime,
    runtime::{ExchangeError, ExchangeFuture, ExchangeRuntime, ExchangeTask},
    websocket::{WebSocketConnectionOptions, WsRuntime, WsSocket},
};
use safe_cex_store::Store;
use safe_cex_transport::{
    RestAdapter, RestFuture, RestRequest, RestResponse, TransportProvider, native::ReqwestAdapter,
    socket::TunnelHost,
};
use safe_cex_registry::{native_host::native_venue_host_with, rust_venues::rust_constructor};
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

const SECRET: &str = "native-host-secret";

#[derive(Debug, Clone)]
struct Seen {
    method: String,
    path: String,
    query: String,
    headers: HashMap<String, String>,
}

fn routes() -> HashMap<String, Value> {
    let corpus: Value = serde_json::from_str(include_str!("../../safe-cex-venue-binance/tests/fixtures/public-rest.json")).unwrap();
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
    ]
    .into_iter()
    .map(|(k, v)| (k.to_owned(), v))
    .collect()
}

/// Minimal HTTP/1.1: one request per connection, recorded, answered from `routes`.
async fn serve_http(
    listener: TcpListener,
    routes: Rc<HashMap<String, Value>>,
    seen: Rc<RefCell<Vec<Seen>>>,
) {
    loop {
        let Ok((mut stream, _)) = listener.accept().await else {
            return;
        };
        let (routes, seen) = (routes.clone(), seen.clone());
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
            seen.borrow_mut().push(Seen {
                method: method.clone(),
                path: path.into(),
                query: query.into(),
                headers,
            });
            let (status, body) = match routes.get(&format!("{method} {path}")) {
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

struct WsPeer {
    path: String,
    push: mpsc::UnboundedSender<String>,
}
async fn serve_ws(
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
struct LocalOrigin {
    inner: ReqwestAdapter,
    origin: String,
}
fn rewrite(url: &str, origin: &str) -> String {
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
struct LocalDial {
    inner: Rc<NativeRuntime>,
    origin: String,
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
    fn console(&self, severity: &str, message: &str) {
        self.inner.console(severity, message)
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
    fn report_raw_exception(&self, error: &Value, context: &Value) {
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

async fn eventually(what: &str, mut ready: impl FnMut() -> bool) {
    for _ in 0..200 {
        if ready() {
            return;
        }
        tokio::time::sleep(Duration::from_millis(10)).await;
    }
    panic!("timed out waiting for {what}");
}
fn query_value<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query
        .split('&')
        .find_map(|pair| pair.strip_prefix(&format!("{key}=")))
}

#[test]
fn binance_futures_runs_on_the_native_host_over_local_http_and_websocket() {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async {
        let http = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let ws = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let http_origin = format!("http://{}", http.local_addr().unwrap());
        let ws_origin = format!("ws://{}", ws.local_addr().unwrap());
        let seen = Rc::new(RefCell::new(vec![]));
        let (peers, received) = (Rc::new(RefCell::new(vec![])), Rc::new(RefCell::new(vec![])));
        tokio::task::spawn_local(serve_http(http, Rc::new(routes()), seen.clone()));
        tokio::task::spawn_local(serve_ws(ws, peers.clone(), received.clone()));

        let host = Rc::new(LocalDial { inner: safe_cex_exchange::native::native_runtime(), origin: ws_origin });
        let adapters = Rc::new(move || {
            let inner = safe_cex_transport::native::reqwest_adapter().map_err(|e| ExchangeError::new(e.to_string()))?;
            Ok(Arc::new(LocalOrigin { inner, origin: http_origin.clone() }) as Arc<dyn RestAdapter>)
        });
        let constructor = rust_constructor(&ExchangeName::Binance, native_venue_host_with(host, adapters))
            .expect("binance is a Rust-constructed venue");
        let options = serde_json::from_value(json!({"key":"native-key","secret":SECRET})).unwrap();
        let store: Rc<dyn Store> = Rc::new(safe_cex_store::store::DefaultStore::new());
        let exchange = constructor
            .construct(ExchangeName::Binance, options, store.clone())
            .expect("registry constructs BinanceExchange on the production native host");

        let markets = exchange.fetch_markets().await.expect("fetchMarkets over HTTP");
        assert!(markets.iter().any(|m| m.symbol == "BTCUSDT"), "exchangeInfo parsed from the wire");
        let validated = exchange.validate_account().await.expect("validateAccount over HTTP");
        assert_eq!(validated.balance.as_ref().map(|b| b.total), Some(100.0));
        assert_eq!(exchange.get_account().await.unwrap().user_id, "alias-native");

        exchange.start().await.expect("start over HTTP and WebSocket");
        eventually("private stream dialed with the listen key", || {
            peers.borrow().iter().any(|p| p.path.starts_with("/private/ws") && p.path.contains("lk-native"))
        })
        .await;
        assert!(store.loaded().markets && store.loaded().tickers, "start loaded public state");
        assert!(seen.borrow().iter().any(|s| s.method == "POST" && s.path == "/fapi/v1/listenKey"));

        let order: safe_cex_types::PlaceOrderOpts = serde_json::from_value(json!({"symbol":"BTCUSDT","type":"limit","side":"buy","amount":1,"price":10})).unwrap();
        let ids = exchange.base().place_order(exchange.as_ref(), order).await.expect("placeOrder over HTTP");
        assert_eq!(ids, vec!["9001"]);
        let placed = seen.borrow().iter().rev().find(|s| s.path == "/fapi/v1/order").cloned().expect("order reached the server");
        assert_eq!(placed.method, "POST");
        assert_eq!(placed.headers.get("x-mbx-apikey").map(String::as_str), Some("native-key"));
        let signature = query_value(&placed.query, "signature").expect("signed query");
        let signed = placed.query.split("&signature=").next().unwrap();
        let mut mac = Hmac::<sha2::Sha256>::new_from_slice(SECRET.as_bytes()).unwrap();
        mac.update(signed.as_bytes());
        let expected: String = mac.finalize().into_bytes().iter().map(|b| format!("{b:02x}")).collect();
        assert_eq!(signature, expected, "server-verified HMAC over the exact bytes sent");
        assert_eq!(query_value(&placed.query, "symbol"), Some("BTCUSDT"));

        let event = json!({"e":"ORDER_TRADE_UPDATE","E":1,"o":{"s":"BTCUSDT","c":"c1","S":"BUY","ot":"LIMIT","q":"1","p":"10","sp":"0","X":"NEW","i":555,"z":"0","R":false,"ap":"0","l":"0"}});
        let private = peers.borrow().iter().find(|p| p.path.starts_with("/private/ws") && p.path.contains("lk-native")).unwrap().push.clone();
        private.send(event.to_string()).unwrap();
        eventually("private ORDER_TRADE_UPDATE reached the store", || {
            store.orders().iter().any(|o| o.id == "555")
        })
        .await;

        exchange.dispose().expect("dispose");
        let _ = received;
    });
}
