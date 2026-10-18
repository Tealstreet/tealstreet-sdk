//! Actual KuCoin receiver, native scheduler, signed HTTP and loopback sockets.
use super::*;
use futures_util::StreamExt;
use safe_cex_exchange::{
    exchange::ExchangeSettings,
    native::{NativeHttpHost, native_runtime},
    socket_lifecycle::SocketSlot,
};
use safe_cex_store::{Store, StoreUpdate, store::DefaultStore};
use safe_cex_transport::{RestRequest, TransportError, TransportFuture};
use safe_cex_types::{ExchangeName, OrderStatus};
use serde_json::{Value, json};
use std::{sync::Arc, time::Duration};
use tokio::io::{AsyncReadExt, AsyncWriteExt};

struct RetryClock(Rc<dyn WsRuntime>);
impl RateLimitClock for RetryClock {
    fn now_ms(&self) -> u64 {
        self.0.now_ms() as u64
    }
    fn sleep_ms(&self, ms: u64) -> TransportFuture<'_, ()> {
        Box::pin(async move {
            self.0
                .sleep(ms)
                .await
                .map_err(|e| TransportError::Host(e.message))
        })
    }
}

struct Fixture {
    venue: Rc<KucoinExchange>,
    store: Rc<DefaultStore>,
    requests: Rc<RefCell<Vec<String>>>,
    dials: Rc<RefCell<Vec<String>>>,
    tasks: Rc<RefCell<Vec<tokio::task::AbortHandle>>>,
}
impl Drop for Fixture {
    fn drop(&mut self) {
        self.venue.dispose().expect("dispose actual KuCoin venue");
        for task in self.tasks.borrow().iter() {
            task.abort();
        }
    }
}
fn own(tasks: &RefCell<Vec<tokio::task::AbortHandle>>, f: impl Future<Output = ()> + 'static) {
    tasks
        .borrow_mut()
        .push(tokio::task::spawn_local(f).abort_handle());
}
async fn wait_for(label: &str, mut ready: impl FnMut() -> bool) {
    tokio::time::timeout(Duration::from_secs(5), async {
        while !ready() {
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await
    .unwrap_or_else(|_| panic!("timed out waiting for {label}"));
}
fn run(test: impl AsyncFnOnce(Fixture)) {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&rt, async {
        test(fixture().await).await;
    });
}
async fn fixture() -> Fixture {
    let http = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let proxy = format!("http://{}", http.local_addr().unwrap());
    let ws = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let endpoint = format!("ws://{}/", ws.local_addr().unwrap());
    let tasks = Rc::new(RefCell::new(vec![]));
    let requests = Rc::new(RefCell::new(vec![]));
    let dials = Rc::new(RefCell::new(vec![]));
    let calls = requests.clone();
    let children = tasks.clone();
    own(&tasks, async move {
        loop {
            let (mut stream, _) = http.accept().await.expect("accept native HTTP request");
            let calls = calls.clone();
            let endpoint = endpoint.clone();
            own(&children, async move {
                let mut bytes = vec![];
                loop {
                    let mut part = [0; 2048];
                    let size = stream
                        .read(&mut part)
                        .await
                        .expect("read native HTTP request");
                    assert!(size > 0, "HTTP peer closed before its headers");
                    bytes.extend_from_slice(&part[..size]);
                    if bytes.windows(4).any(|v| v == b"\r\n\r\n") {
                        break;
                    }
                }
                let request = String::from_utf8(bytes).unwrap();
                let path = request
                    .lines()
                    .next()
                    .unwrap()
                    .split_whitespace()
                    .nth(1)
                    .unwrap()
                    .to_owned();
                calls.borrow_mut().push(path.clone());
                let path = path.split('?').next().unwrap();
                let data = if path.ends_with("/bullet-private") || path.ends_with("/bullet-public")
                {
                    let number = calls
                        .borrow()
                        .iter()
                        .filter(|v| v.split('?').next() == Some(path))
                        .count();
                    json!({"token":format!("token-{number}"),"instanceServers":[{"endpoint":endpoint,"pingInterval":18000}]})
                } else if path.ends_with("/orders") || path.ends_with("/stopOrders") {
                    json!({"items":[]})
                } else {
                    panic!("unexpected actual HTTP request: {path}");
                };
                let body = json!({"code":"200000","data":data}).to_string();
                stream.write_all(format!("HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",body.len()).as_bytes()).await.expect("write native HTTP response");
            });
        }
    });
    let urls = dials.clone();
    let children = tasks.clone();
    own(&tasks, async move {
        loop {
            let (stream, _) = ws.accept().await.expect("accept native websocket");
            let urls = urls.clone();
            own(&children, async move {
                let mut socket = tokio_tungstenite::accept_hdr_async(
                    stream,
                    move |request: &tokio_tungstenite::tungstenite::handshake::server::Request,
                          response| {
                        urls.borrow_mut().push(request.uri().to_string());
                        Ok(response)
                    },
                )
                .await
                .expect("native websocket handshake");
                while let Some(message) = socket.next().await {
                    match message {
                        Ok(tokio_tungstenite::tungstenite::Message::Close(_)) => break,
                        Ok(_) => {},
                        Err(tokio_tungstenite::tungstenite::Error::Protocol(tokio_tungstenite::tungstenite::error::ProtocolError::ResetWithoutClosingHandshake)) => break,
                        Err(error) => panic!("native websocket peer: {error}"),
                    }
                }
            });
        }
    });
    let runtime = native_runtime();
    let events = Rc::new(RefCell::new(None));
    let host = Rc::new(NativeHttpHost {
        events: events.clone(),
    });
    let adapter = Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap());
    let rt = runtime.clone();
    let store = Rc::new(DefaultStore::new());
    Store::update(
        store.as_ref(),
        StoreUpdate {
            markets: Some(vec![
                serde_json::from_value(json!({
                    "id":"XBTUSDTM","symbol":"BTCUSDT","base":"BTC","quote":"USDT","active":true,
                    "contractSize":0.001,"precision":{"amount":1,"price":0.1},
                    "limits":{"amount":{"min":1,"max":100000},"leverage":{"min":1,"max":100}}
                }))
                .unwrap(),
            ]),
            ..Default::default()
        },
        false,
    )
    .unwrap();
    let venue = KucoinExchange::new(BaseExchangeParts {
        name: ExchangeName::Kucoin,
        options: serde_json::from_value(json!({"key":"fixture","secret":"fixture","passphrase":"fixture","overrideProxyUrl":proxy})).unwrap(),
        store: store.clone(),
        settings: ExchangeSettings { supports_public_only: true, positions_supported: Some(true), ..Default::default() },
        external_public_data: None,
    }, KucoinDependencies {
        runtime: runtime.clone(), hosted_signer: None,
        http: Rc::new(move |options| HttpClientOptions {
            params_serializer: None, response_transform: None, exchange: ExchangeName::Kucoin,
            options: options.clone(), request: RestRequest::default(), provider: Some(rt.transport()),
            signer: None, account_name: None, adapter: adapter.clone(), host: host.clone(), runtime: rt.clone(),
            store: None, stats: None, logs: None, every: vec![], periodic: vec![],
            next_id: Arc::new(safe_cex_exchange::native::nanoid),
            signing_now: Arc::new(|| safe_cex_transport::native::wall_clock_ms() as u64),
        }),
        registry: Rc::new(TransportRegistry::default()), clock: VirtualClock::default(), retry_clock: Rc::new(RetryClock(runtime)),
    }).unwrap();
    let erased: Rc<dyn Exchange> = venue.clone();
    *events.borrow_mut() = Some(Rc::downgrade(&erased));
    Fixture {
        venue,
        store,
        requests,
        dials,
        tasks,
    }
}
async fn private_frame(f: &Fixture, value: Value) {
    let socket = f.venue.base().get_socket(SocketSlot::Private).unwrap();
    if !socket.is_connected() {
        socket.connect_and_subscribe(false).unwrap();
        wait_for("private frame connection", || socket.is_connected()).await;
    }
    socket
        .on_message(&safe_cex_transport::socket::SocketFrame::Text(
            value.to_string(),
        ))
        .unwrap();
}

fn order(kind: &str, filled: u32, matched: Option<u32>) -> Value {
    let mut value = json!({"type":"message","topic":"/contractMarket/tradeOrders","data":{
        "orderId":"o1","symbol":"XBTUSDTM","type":kind,"side":"buy","price":"95","size":10,
        "filledSize":filled,"remainSize":10-filled,"status":if filled==10 {"done"} else {"open"},
        "orderType":"limit","timeInForce":"GTC","reduceOnly":false,"createdAt":1700000000000u64,"stop":""
    }});
    if let Some(size) = matched {
        value["data"]["matchSize"] = json!(size);
        value["data"]["matchPrice"] = json!(96);
    }
    value
}
async fn renew_session(f: Fixture, slot: SocketSlot, member: &str) {
    let socket = f.venue.base().get_socket(slot).unwrap();
    socket.connect_and_subscribe(false).unwrap();
    wait_for("initial KuCoin connection", || socket.is_connected()).await;
    let receiver = f.venue.receiver.this.get(member).unwrap();
    assert_eq!(receiver.get("token").unwrap().to_display(), "token-1");
    let output = std::process::Command::new("node")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/reconnect-source.cjs"
        ))
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    let oracle: Value = serde_json::from_slice(&output.stdout).unwrap();
    let expected = &oracle[if member == "privateWebsocket" {
        "private"
    } else {
        "public"
    }];
    socket.reconnect().unwrap();
    assert!(
        matches!(receiver.get("token").unwrap(), Js::Null),
        "typed reconnect must run the source token reset before awaiting HTTP"
    );
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_json(&receiver.get("token").unwrap()).unwrap(),
        expected["token"]
    );
    assert_eq!(
        safe_cex_exchange::js_runtime::value::to_json(&receiver.get("socketEndpoint").unwrap())
            .unwrap(),
        expected["endpoint"]
    );
    if member == "privateWebsocket" {
        assert_eq!(
            safe_cex_exchange::js_runtime::value::to_json(&receiver.get("subscribed").unwrap())
                .unwrap(),
            expected["subscribed"]
        );
    }
    wait_for("fresh KuCoin token", || {
        receiver.get("token").unwrap().to_display() == "token-2" && socket.is_connected()
    })
    .await;
    assert_eq!(f.dials.borrow().len(), 2);
    assert_eq!(
        f.dials
            .borrow()
            .iter()
            .filter(|u| u.contains("token=token-2"))
            .count(),
        1
    );
}
#[test]
fn typed_public_reconnect_renews_kucoin_session() {
    run(async |f| renew_session(f, SocketSlot::Public, "publicWebsocket").await);
}
#[test]
fn typed_private_reconnect_renews_kucoin_session() {
    run(async |f| renew_session(f, SocketSlot::Private, "privateWebsocket").await);
}
#[test]
fn native_private_order_frame_commits_source_store_row() {
    run(async |f| {
        private_frame(&f, order("open", 0, None)).await;
        assert_eq!(f.store.orders().len(), 1);
        let row = &f.store.orders()[0];
        assert_eq!(row.id, "o1");
        assert_eq!(row.symbol, "BTCUSDT");
        assert_eq!(row.status, OrderStatus::Open);
        assert_eq!(row.amount, 0.01);
    });
}
#[test]
fn native_private_match_emits_incremental_base_fill() {
    run(async |f| {
        let fills = Rc::new(RefCell::new(vec![]));
        let seen = fills.clone();
        f.venue.base().on(
            "fill",
            Rc::new(move |_, v| {
                seen.borrow_mut().push(v.clone());
                Ok(())
            }),
        );
        private_frame(&f, order("open", 0, None)).await;
        private_frame(&f, order("match", 4, Some(4))).await;
        private_frame(&f, order("filled", 10, None)).await;
        assert_eq!(f.store.orders()[0].status, OrderStatus::Closed);
        assert_eq!(f.store.orders()[0].filled, 0.004);
        let fills = fills.borrow();
        assert_eq!(fills.len(), 1);
        assert_eq!(fills[0].own_keys().len(), 5);
        for (key, value) in [
            ("id", Js::str("o1")),
            ("side", Js::str("buy")),
            ("symbol", Js::str("BTCUSDT")),
            ("price", Js::Number(96.0)),
            ("amount", Js::Number(0.004)),
        ] {
            assert!(fills[0].get(key).unwrap().strict_equals(&value), "{key}");
        }
    });
}
fn seed_private_state(f: &Fixture) {
    Store::update(f.store.as_ref(), StoreUpdate {
        positions: Some(vec![serde_json::from_value(json!({"id":"p1","symbol":"BTCUSDT","side":"long","entryPrice":99,"notional":0.5,"leverage":10,"unrealizedPnl":1.5,"contracts":0.005,"liquidationPrice":50})).unwrap()]),
        balance: Some(serde_json::from_value(json!({"free":70,"used":5,"total":100,"upnl":1.5})).unwrap()),
        ..Default::default()
    }, false).unwrap();
}
#[test]
fn native_private_position_frame_runs_source_orders_snapshot() {
    run(async |f| {
        seed_private_state(&f);
        let positions = f.store.positions().to_vec();
        private_frame(&f,json!({"type":"message","topic":"/contract/position:XBTUSDTM","data":{"changeReason":"positionChange"}})).await;
        wait_for("position topic orders snapshot", || f.store.loaded().orders).await;
        assert_eq!(
            serde_json::to_value(f.store.positions().to_vec()).unwrap(),
            serde_json::to_value(positions).unwrap()
        );
        assert!(f.store.orders().is_empty());
        assert!(
            f.requests
                .borrow()
                .iter()
                .any(|p| p.contains("/api/v1/orders"))
        );
    });
}
#[test]
fn native_private_balance_frame_runs_source_orders_snapshot() {
    run(async |f| {
        seed_private_state(&f);
        private_frame(
            &f,
            json!({"type":"message","topic":"/contractAccount/wallet","data":{"currency":"USDT"}}),
        )
        .await;
        wait_for("wallet topic orders snapshot", || f.store.loaded().orders).await;
        assert_eq!(f.store.balance().free, 70.0);
        assert_eq!(f.store.balance().used, 5.0);
        assert_eq!(f.store.balance().total, 100.0);
        assert_eq!(f.store.balance().upnl, 1.5);
        assert!(f.store.orders().is_empty());
        assert!(
            f.requests
                .borrow()
                .iter()
                .any(|p| p.contains("/api/v1/orders"))
        );
    });
}
