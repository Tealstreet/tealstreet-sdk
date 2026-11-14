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
    pub refused: usize,
    pub writes: Vec<Value>,
}

#[derive(Clone)]
pub(crate) struct MockVenue {
    key: String,
    material: String,
    pub observed: Arc<Mutex<Observed>>,
    admission: Option<crate::delegated::probe::venue_tests::VenueIo>,
}

impl MockVenue {
    pub fn new(key: String, material: String) -> Self {
        Self {
            key,
            material,
            observed: Arc::default(),
            admission: None,
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
        if let Some(io) = &self.admission {
            return io.prepare_http(request, intent);
        }
        self.observed.lock().unwrap().preparations += 1;
        let venue = self.clone();
        Box::pin(async move { Ok(Box::new(Prepared(venue)) as Box<dyn PreparedHttp>) })
    }

    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
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
        if let Some(io) = &self.admission {
            return io.request(request, events);
        }
        self.observed.lock().unwrap().requests += 1;
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
