//! In-memory venue I/O for the server's end-to-end tests. Not built in the server.
use safe_cex_engine::{HttpRequest, Method, RequestId, SocketId};
use safe_cex_native::{
    BoundHttp, Events, HostEvent, HttpPrepareError, HttpPrepareFuture, NetworkTransport,
    PreparedHttp, Transport,
};
use serde_json::{Value, json};
use std::sync::{Arc, Mutex};
use tokio::sync::mpsc;

#[derive(Default)]
pub(crate) struct Observed {
    pub preparations: usize,
    pub requests: usize,
    pub sockets: usize,
    pub reads: usize,
    pub registration_urls: Vec<String>,
    pub refused: usize,
    pub writes: Vec<Value>,
}

#[derive(Clone)]
pub(crate) struct MockVenue {
    key: String,
    material: String,
    pub observed: Arc<Mutex<Observed>>,
    admission: Option<crate::delegated::probe::venue_tests::VenueIo>,
    registration: Option<Registration>,
    coinbase: Option<crate::delegated::probe::coinbase_tests::CoinbaseIo>,
    hyperliquid: bool,
    placement_fixture: bool,
}
#[derive(Clone)]
struct Registration {
    body: Arc<Mutex<Value>>,
    gate: Arc<tokio::sync::Semaphore>,
}

impl MockVenue {
    pub fn new(key: String, material: String) -> Self {
        Self {
            key,
            material,
            observed: Arc::default(),
            admission: None,
            registration: None,
            coinbase: None,
            hyperliquid: false,
            placement_fixture: false,
        }
    }

    #[cfg(target_os = "linux")]
    pub(crate) fn for_hyperliquid(key: String, material: String) -> Self {
        Self {
            hyperliquid: true,
            ..Self::new(key, material)
        }
    }

    pub(crate) fn for_coinbase(io: crate::delegated::probe::coinbase_tests::CoinbaseIo) -> Self {
        Self {
            coinbase: Some(io),
            ..Self::new("synthetic".into(), "synthetic".into())
        }
    }

    pub(crate) fn for_admission(
        venue: &'static str,
    ) -> (Self, Arc<Mutex<crate::delegated::probe::venue_tests::Seen>>) {
        let seen = Arc::default();
        let mut mock = Self::new("synthetic-key".into(), "synthetic-material".into());
        mock.admission = Some(crate::delegated::probe::venue_tests::VenueIo {
            venue,
            seen: Arc::clone(&seen),
        });
        (mock, seen)
    }

    pub(crate) fn for_placement(
        venue: &'static str,
    ) -> (Self, Arc<Mutex<crate::delegated::probe::venue_tests::Seen>>) {
        let (mut mock, seen) = Self::for_admission(venue);
        mock.placement_fixture = true;
        (mock, seen)
    }

    pub(crate) fn registered_key(body: Value, gate: Arc<tokio::sync::Semaphore>) -> Self {
        Self::registered_key_state(Arc::new(Mutex::new(body)), gate)
    }

    pub(crate) fn registered_key_state(
        body: Arc<Mutex<Value>>,
        gate: Arc<tokio::sync::Semaphore>,
    ) -> Self {
        Self {
            registration: Some(Registration { body, gate }),
            ..Self::new("synthetic".into(), "synthetic".into())
        }
    }

    fn accepts(&self, request: &HttpRequest) -> bool {
        let header = |name: &str| {
            request
                .headers
                .iter()
                .find(|(n, _)| n.eq_ignore_ascii_case(name))
                .map(|(_, v)| v.as_str())
                .unwrap_or("")
        };
        let payload = if request.method == Method::Get {
            request.url.split_once('?').map_or("", |(_, query)| query)
        } else {
            request.body.as_deref().unwrap_or("")
        };
        let canonical = format!(
            "{}{}{}{}",
            header("X-BAPI-TIMESTAMP"),
            self.key,
            header("X-BAPI-RECV-WINDOW"),
            payload
        );
        let expected = safe_cex_crypto::hmac_sha256(
            self.material.as_bytes(),
            canonical.as_bytes(),
            safe_cex_crypto::Encoding::Hex,
        );
        header("X-BAPI-API-KEY") == self.key && header("X-BAPI-SIGN") == expected
    }
}

fn answer(events: &Events, id: RequestId, body: Value) {
    let _ = events.send(HostEvent::HttpStart {
        request: id,
        status: 200,
        headers: String::new(),
    });
    let _ = events.send(HostEvent::HttpChunk(id, body.to_string().into_bytes()));
    let _ = events.send(HostEvent::HttpEnd(id));
}

impl Transport for MockVenue {
    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        if let Some(io) = &self.coinbase {
            return io.prepare_http(request, intent);
        }
        if !self.placement_fixture
            && let Some(io) = &self.admission
        {
            return io.prepare_http(request, intent);
        }
        self.observed.lock().unwrap().preparations += 1;
        let venue = self.clone();
        Box::pin(async move {
            if let Some(registration) = &venue.registration {
                venue
                    .observed
                    .lock()
                    .unwrap()
                    .registration_urls
                    .push(request.url.clone());
                assert_eq!(
                    intent,
                    safe_cex_engine::request_context::RoutingIntent::Public
                );
                assert_eq!(request.method, Method::Get);
                assert!(
                    request
                        .url
                        .ends_with("/api/v1/apikeys?account_index=42&api_key_index=1")
                );
                assert!(request.headers.is_empty() && request.body.is_none());
                registration.gate.acquire().await.unwrap().forget();
            }
            Ok(Box::new(Prepared(venue)) as Box<dyn PreparedHttp>)
        })
    }

    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        if let Some(io) = &self.coinbase {
            return io.connect(socket, url, events);
        }
        if let Some(io) = &self.admission {
            return io.connect(socket, url, events);
        }
        self.observed.lock().unwrap().sockets += 1;
        let (tx, mut rx) = mpsc::unbounded_channel::<String>();
        let _ = events.send(HostEvent::WsOpened(socket));
        tokio::task::spawn_local(async move {
            while let Some(text) = rx.recv().await {
                let value: Value = serde_json::from_str(&text).unwrap();
                let response = json!({"success":true,"op":value["op"],"ret_msg":""});
                let _ = events.send(HostEvent::WsMessage(socket, response.to_string()));
            }
        });
        tx
    }

    fn request(&self, request: HttpRequest, events: Events) {
        if let Some(io) = &self.coinbase {
            io.request(request, events);
            return;
        }
        if self.placement_fixture && request.method == Method::Post {
            let io = self.admission.as_ref().unwrap();
            let path = request.url.split('?').next().unwrap();
            if path.contains("/order/") || path.contains("/trade/") || path.ends_with("/um/order") {
                assert!(io.valid_private_signature(&request), "protected write HMAC");
                let body: Value = if let Some(body) = &request.body {
                    serde_json::from_str(body).unwrap()
                } else {
                    request
                        .url
                        .split_once('?')
                        .unwrap()
                        .1
                        .split('&')
                        .map(|field| {
                            let (key, value) = field.split_once('=').unwrap();
                            (key.to_owned(), Value::String(value.to_owned()))
                        })
                        .collect::<serde_json::Map<String, Value>>()
                        .into()
                };
                let rows = body
                    .get("orderList")
                    .and_then(Value::as_array)
                    .or_else(|| body.as_array())
                    .cloned()
                    .unwrap_or_else(|| vec![body.clone()]);
                assert_eq!(rows.len(), 1, "one native order, no split fallback");
                let client = match io.venue {
                    "bitget" => &rows[0]["clientOid"],
                    "okx" => &rows[0]["clOrdId"],
                    "blofin" => &rows[0]["clientOrderId"],
                    "binancepm" => &rows[0]["newClientOrderId"],
                    other => panic!("no placement fixture for {other}"),
                };
                assert!(client.as_str().is_some_and(|id| !id.is_empty()));
                self.observed.lock().unwrap().writes.push(body);
                let response = match io.venue {
                    "bitget" => {
                        json!({"code":"00000","msg":"success","data":{"successList":[{"orderId":"77","clientOid":client}],"failureList":[]}})
                    }
                    "okx" => {
                        json!({"code":"0","msg":"","data":[{"ordId":"77","clOrdId":client,"sCode":"0","sMsg":""}]})
                    }
                    "blofin" => {
                        json!({"code":"0","msg":"","data":[{"orderId":"77","clientOrderId":client,"code":"0","msg":""}]})
                    }
                    "binancepm" => {
                        json!({"orderId":77,"clientOrderId":client,"symbol":"BTCUSDT","status":"NEW"})
                    }
                    _ => unreachable!(),
                };
                answer(&events, request.id, response);
                return;
            }
        }
        if let Some(io) = &self.admission {
            return io.request(request, events);
        }
        self.observed.lock().unwrap().requests += 1;
        if let Some(registration) = &self.registration {
            self.observed.lock().unwrap().reads += 1;
            answer(
                &events,
                request.id,
                registration.body.lock().unwrap().clone(),
            );
            return;
        }
        if self.hyperliquid {
            let body: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            let reply = if request.url.ends_with("/exchange") {
                assert_eq!(body["action"]["type"], "order");
                assert!(body["nonce"].as_u64().is_some());
                for part in ["r", "s"] {
                    let text = body["signature"][part].as_str().unwrap();
                    assert_eq!(text.len(), 66);
                    assert!(text.starts_with("0x"));
                    assert!(text[2..].bytes().all(|b| b.is_ascii_hexdigit()));
                }
                assert!(matches!(body["signature"]["v"].as_u64(), Some(27 | 28)));
                self.observed.lock().unwrap().writes.push(body);
                json!({"status":"ok","response":{"type":"order","data":{"statuses":[{"resting":{"oid":77}}]}}})
            } else {
                assert!(request.url.ends_with("/info"));
                self.observed.lock().unwrap().reads += 1;
                match body["type"].as_str().unwrap() {
                    "metaAndAssetCtxs" => json!([
                        {"universe":[{"name":"BTC","szDecimals":5,"maxLeverage":50}]},
                        [{"midPx":"55000","markPx":"55000","oraclePx":"55000","prevDayPx":"54000"}]
                    ]),
                    "perpDexs" => json!([null]),
                    "userAbstraction" => json!("disabled"),
                    "maxBuilderFee" => json!(30),
                    "userFees" => json!({"userCrossRate":"0.00045","userAddRate":"0.00015"}),
                    "clearinghouseState" => {
                        assert_eq!(body["user"], self.key);
                        json!({"assetPositions":[],"marginSummary":{"accountValue":"1000","totalMarginUsed":"0"},"time":1})
                    }
                    "spotClearinghouseState" => json!({"balances":[]}),
                    "frontendOpenOrders" => json!([]),
                    other => panic!("unexpected Hyperliquid fixture read {other}"),
                }
            };
            answer(&events, request.id, reply);
            return;
        }
        let path = request.url.split('?').next().unwrap();
        let private = request
            .headers
            .iter()
            .any(|(name, _)| name.eq_ignore_ascii_case("X-BAPI-API-KEY"));
        let write = path.contains("/v5/order/create");
        if private {
            let mut observed = self.observed.lock().unwrap();
            if !self.accepts(&request) {
                observed.refused += 1;
                answer(
                    &events,
                    request.id,
                    json!({"retCode":10003,"retMsg":"synthetic refusal","result":{}}),
                );
                return;
            }
            if !write {
                observed.reads += 1;
            }
        }
        if write {
            assert!(
                private && self.accepts(&request),
                "manual write must be resident-signed"
            );
            let payload: Value = serde_json::from_str(request.body.as_deref().unwrap()).unwrap();
            let submitted = if path.ends_with("-batch") {
                payload["request"].as_array().unwrap().clone()
            } else {
                vec![payload]
            };
            self.observed
                .lock()
                .unwrap()
                .writes
                .extend(submitted.clone());
            let rows: Vec<Value> = submitted.iter().map(|row| json!({"orderId":"synthetic-order","orderLinkId":row["orderLinkId"],"symbol":row["symbol"]})).collect();
            let codes: Vec<Value> = rows.iter().map(|_| json!({"code":0,"msg":"OK"})).collect();
            answer(
                &events,
                request.id,
                if path.ends_with("-batch") {
                    json!({"retCode":0,"retMsg":"OK","result":{"list":rows},"retExtInfo":{"list":codes}})
                } else {
                    json!({"retCode":0,"retMsg":"OK","result":{"orderId":"synthetic-order"}})
                },
            );
            return;
        }
        let fixtures: Value = serde_json::from_str(include_str!(
            "../tests/fixtures/delegated-bybit-public.json"
        ))
        .unwrap();
        let body = if path.ends_with("/market/instruments-info")
            && !request.url.contains("PreLaunch")
        {
            fixtures["instruments"].clone()
        } else if path.ends_with("/market/tickers") {
            fixtures["tickers"].clone()
        } else if path.ends_with("/account/info") {
            json!({"retCode":0,"result":{"marginMode":"REGULAR_MARGIN","unifiedMarginStatus":4}})
        } else if path.ends_with("/account/wallet-balance") {
            json!({"retCode":0,"result":{"list":[{"accountType":"UNIFIED","totalEquity":"10000","totalWalletBalance":"10000","totalMarginBalance":"10000","totalAvailableBalance":"10000","totalPerpUPL":"0","coin":[{"coin":"USDT","walletBalance":"10000","equity":"10000","usdValue":"10000","availableToWithdraw":"10000","unrealisedPnl":"0","locked":"0"}]}]}})
        } else if path.ends_with("/position/list") {
            json!({"retCode":0,"result":{"list":[{"symbol":"BTCUSDT","positionIdx":0,"side":"Buy","size":"0.001","leverage":"10","avgPrice":"50000","markPrice":"50000","positionValue":"50","positionIM":"5","unrealisedPnl":"0","cumRealisedPnl":"0","liqPrice":"0"}],"nextPageCursor":""}})
        } else {
            json!({"retCode":0,"retMsg":"OK","result":{"list":[],"nextPageCursor":""}})
        };
        answer(&events, request.id, body);
    }
}

struct Prepared(MockVenue);
struct Bound(MockVenue, HttpRequest);
impl std::fmt::Debug for Prepared {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("PreparedMock")
    }
}
impl std::fmt::Debug for Bound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BoundMock")
    }
}
impl PreparedHttp for Prepared {
    fn bind(self: Box<Self>, packet: HttpRequest) -> Result<Box<dyn BoundHttp>, HttpPrepareError> {
        Ok(Box::new(Bound(self.0, packet)))
    }
}
impl BoundHttp for Bound {
    fn ready(&self) -> bool {
        true
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        self.0.request(self.1, events);
        tokio::spawn(async {}).abort_handle()
    }
}

/// Default tests still delegate every operation to their original transport.
pub(crate) enum Io {
    Network(NetworkTransport),
    Mock(MockVenue),
}
impl Io {
    pub fn new(network: NetworkTransport, mock: Option<MockVenue>) -> Self {
        mock.map_or(Self::Network(network), Self::Mock)
    }
    fn inner(&self) -> &dyn Transport {
        match self {
            Self::Network(t) => t,
            Self::Mock(t) => t,
        }
    }
}
impl Transport for Io {
    fn prepare_http(
        &self,
        r: HttpRequest,
        i: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        self.inner().prepare_http(r, i)
    }
    fn prepare_http_classified(
        &self,
        r: HttpRequest,
        i: safe_cex_engine::request_context::RoutingIntent,
        c: safe_cex_engine::request_budget::RequestClass,
    ) -> HttpPrepareFuture {
        self.inner().prepare_http_classified(r, i, c)
    }
    fn connect(&self, s: SocketId, u: String, e: Events) -> mpsc::UnboundedSender<String> {
        self.inner().connect(s, u, e)
    }
    fn connect_with_headers(
        &self,
        s: SocketId,
        u: String,
        h: Vec<(&'static str, String)>,
        e: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner().connect_with_headers(s, u, h, e)
    }
    fn connect_channel(
        &self,
        s: SocketId,
        u: String,
        h: Vec<(&'static str, String)>,
        c: Option<safe_cex_engine::SocketChannel>,
        e: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner().connect_channel(s, u, h, c, e)
    }
    fn connect_context(
        &self,
        s: SocketId,
        u: String,
        h: Vec<(&'static str, String)>,
        c: Option<safe_cex_engine::SocketChannel>,
        x: Option<safe_cex_engine::SocketContext>,
        e: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner().connect_context(s, u, h, c, x, e)
    }
    #[allow(clippy::too_many_arguments)]
    fn connect_encoding(
        &self,
        b: bool,
        s: SocketId,
        u: String,
        h: Vec<(&'static str, String)>,
        c: Option<safe_cex_engine::SocketChannel>,
        x: Option<safe_cex_engine::SocketContext>,
        e: Events,
    ) -> mpsc::UnboundedSender<String> {
        self.inner().connect_encoding(b, s, u, h, c, x, e)
    }
    fn request(&self, r: HttpRequest, e: Events) {
        self.inner().request(r, e)
    }
}
