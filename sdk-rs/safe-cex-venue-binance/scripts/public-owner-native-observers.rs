use crate::private_event_value::EventOps;
use crate::{
    exchange_classes::ExchangeOwner,
    public_classes::{PublicKind, PublicSocket},
    public_listeners::{ListenerHost, Parser},
};
use safe_cex_exchange::native::native_runtime;
use serde_json::{Value, json};
use std::{cell::RefCell, future::Future, rc::Rc, sync::Arc};

pub struct Fixture {
    pub owner: Rc<ExchangeOwner>,
    pub socket: Rc<PublicSocket>,
}

pub fn run<F: Future<Output = Value>>(
    kind: PublicKind,
    observe: impl FnOnce(Fixture) -> F,
) -> Value {
    run_with_http(
        kind,
        Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        observe,
    )
}

pub fn run_with_http<F: Future<Output = Value>>(
    kind: PublicKind,
    http: Arc<dyn safe_cex_transport::RestAdapter>,
    observe: impl FnOnce(Fixture) -> F,
) -> Value {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .unwrap();
    tokio::task::LocalSet::new().block_on(&runtime, async move {
        let venue = match kind {
            PublicKind::FuturesMarket | PublicKind::FuturesPublic => "binance",
            PublicKind::PMMarket | PublicKind::PMPublic => "binancepm",
            PublicKind::SpotPublic => "binancespot",
        };
        let selector = match kind {
            PublicKind::FuturesMarket | PublicKind::FuturesPublic => {
                crate::private_session::Venue::Futures
            }
            PublicKind::PMMarket | PublicKind::PMPublic => crate::private_session::Venue::PM,
            PublicKind::SpotPublic => crate::private_session::Venue::Spot,
        };
        let deps = crate::native_host::native_dependencies(
            native_runtime(),
            http,
            Rc::new(|_, _| panic!("public owner receipt must not use external data")),
            None,
            safe_cex_exchange::native::NativeHttpEvents::default(),
        );
        let owner = ExchangeOwner::new(
            selector,
            serde_json::from_value(json!(venue)).unwrap(),
            serde_json::from_value(json!({"key":"receipt","secret":"receipt"})).unwrap(),
            Rc::new(safe_cex_store::store::DefaultStore::new()),
            deps,
        )
        .unwrap();
        let public = owner.public_socket();
        let socket = if matches!(kind, PublicKind::FuturesMarket | PublicKind::PMMarket) {
            public.market_websocket.get().unwrap().clone()
        } else {
            public
        };
        assert_eq!(socket.kind, kind);
        let retained = owner.clone();
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            observe(Fixture { owner, socket }),
        )
        .await
        .expect("native public owner observation timed out");
        retained.public_socket().dispose();
        result
    })
}

pub fn parser() -> Parser {
    Rc::new(|_| Ok(()))
}

pub fn handlers(
    key: String,
    actual: impl FnOnce(&PublicSocket, &str) -> Option<Vec<Parser>>,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        let original = parser();
        fixture
            .socket
            .replace_handlers("receipt".into(), vec![original.clone()]);
        let found = actual(&fixture.socket, &key);
        if let Some(parsers) = &found {
            assert_eq!(parsers.len(), 1);
            assert!(Rc::ptr_eq(&parsers[0], &original));
        }
        json!({"count":found.map(|p|p.len())})
    })
}

pub fn replace(count: usize, actual: impl FnOnce(&PublicSocket, String, Vec<Parser>)) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        let original = (0..count).map(|_| parser()).collect::<Vec<_>>();
        actual(&fixture.socket, "receipt".into(), original.clone());
        let found = fixture.socket.handlers("receipt").unwrap();
        assert!(found.iter().zip(&original).all(|(a, b)| Rc::ptr_eq(a, b)));
        let core_count = fixture.socket.base().message_handlers.borrow()["receipt"]
            .borrow()
            .len();
        json!({"nativeCount":found.len(),"coreCount":core_count})
    })
}

pub fn delete(key: String, actual: impl FnOnce(&PublicSocket, &str)) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        fixture
            .socket
            .replace_handlers("receipt".into(), vec![parser()]);
        fixture
            .socket
            .replace_handlers("keep".into(), vec![parser()]);
        actual(&fixture.socket, &key);
        assert_eq!(fixture.socket.handlers("keep").unwrap().len(), 1);
        json!({"receiptPresent":fixture.socket.handlers("receipt").is_some()})
    })
}

pub fn cleanup(
    key: String,
    actual: impl FnOnce(&PublicSocket, &str) -> Result<(), crate::request_weight::Failure>,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        fixture
            .socket
            .replace_handlers("receipt".into(), vec![parser()]);
        fixture
            .socket
            .replace_handlers("keep".into(), vec![parser()]);
        fixture
            .socket
            .base()
            .record_message_handler_stats("receipt")
            .unwrap();
        fixture
            .socket
            .base()
            .record_message_handler_stats("keep")
            .unwrap();
        actual(&fixture.socket, &key).unwrap();
        let base = fixture.socket.base();
        let stats = base.message_handlers_stats.borrow();
        assert!(stats.contains_key("keep"));
        json!({"receiptPresent":stats.contains_key("receipt")})
    })
}

pub fn seed_ticker(fixture: &Fixture) {
    let ticker = serde_json::from_value(json!({"id":"receipt","symbol":"BTCUSDT","bid":1,"ask":2,"last":3,"mark":4,"index":5,"percentage":6,"fundingRate":7,"fundingTime":8,"volume":9,"quoteVolume":10,"openInterest":11})).unwrap();
    fixture
        .socket
        .parent
        .store()
        .update(
            safe_cex_store::StoreUpdate {
                tickers: Some(vec![ticker]),
                ..Default::default()
            },
            false,
        )
        .unwrap();
}

pub fn ticker_bits(fixture: &Fixture) -> Value {
    let store = fixture.socket.parent.store();
    let tickers = store.tickers();
    let ticker = &tickers[0];
    assert_eq!(ticker.id, "receipt");
    assert_eq!(ticker.symbol, "BTCUSDT");
    assert_eq!(ticker.last.to_bits(), 3.0_f64.to_bits());
    json!({"bidBits":format!("{:016x}",ticker.bid.to_bits()),"askBits":format!("{:016x}",ticker.ask.to_bits())})
}

pub fn apply(
    update: Value,
    actual: impl FnOnce(
        &PublicSocket,
        crate::public_listeners::Book,
        crate::private_event_value::EventValue,
        crate::public_listeners::SyncState,
    ) -> Result<
        crate::protocol_types::BinanceOrderBookApplyResult,
        crate::request_weight::Failure,
    >,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        let book = Rc::new(RefCell::new(safe_cex_types::OrderBook {
            bids: vec![],
            asks: vec![],
        }));
        let sync = Rc::new(RefCell::new(crate::orderbook::BinanceOrderBookSyncState {
            snapshot_last_update_id: Some(17.),
            last_applied_update_id: None,
        }));
        let result = actual(
            &fixture.socket,
            book.clone(),
            safe_cex_exchange::js_runtime::value::from_json(&update),
            sync.clone(),
        )
        .unwrap();
        use crate::protocol_types::BinanceOrderBookApplyResult as Result;
        let tag = match result {
            Result::Applied => "applied",
            Result::Gap => "gap",
            Result::Ignored => "ignored",
        };
        let book = book.borrow();
        let bids = book
            .bids
            .iter()
            .map(|row| {
                [
                    format!("{:016x}", row.price.to_bits()),
                    format!("{:016x}", row.amount.to_bits()),
                ]
            })
            .collect::<Vec<_>>();
        let asks = book
            .asks
            .iter()
            .map(|row| {
                [
                    format!("{:016x}", row.price.to_bits()),
                    format!("{:016x}", row.amount.to_bits()),
                ]
            })
            .collect::<Vec<_>>();
        json!({"result":tag,"bids":bids,"asks":asks,
            "lastIdBits":sync.borrow().last_applied_update_id.map(|n|format!("{:016x}",n.to_bits()))})
    })
}

pub fn message(
    wire: String,
    actual: impl FnOnce(&PublicSocket, &str) -> Result<(), crate::request_weight::Failure>,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        seed_ticker(&fixture);
        match actual(&fixture.socket, &wire) {
            Ok(()) => ticker_bits(&fixture),
            Err(error) => json!({"name": error.name, "message": error.message}),
        }
    })
}

pub fn stream(
    events: Value,
    actual: impl FnOnce(
        &PublicSocket,
        &str,
        crate::private_event_value::EventValue,
    ) -> Result<(), crate::request_weight::Failure>,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        seed_ticker(&fixture);
        let input = safe_cex_exchange::js_runtime::value::from_json(&events);
        match actual(&fixture.socket, "bookTicker", input) {
            Ok(()) => ticker_bits(&fixture),
            Err(error) => json!({"name": error.name, "message": error.message}),
        }
    })
}

pub fn listen(
    symbol: String,
    actual: impl FnOnce(
        &Rc<crate::public_classes::PublicState>,
        String,
        Rc<dyn Fn(crate::public_listeners::Book) -> Result<(), crate::request_weight::Failure>>,
    )
        -> Result<crate::public_listeners::Unsubscribe, crate::request_weight::Failure>,
) -> Value {
    run(PublicKind::FuturesPublic, |fixture| async move {
        let state: &Rc<crate::public_classes::PublicState> = &fixture.socket;
        let callbacks = Rc::new(std::cell::Cell::new(0));
        let seen = callbacks.clone();
        let result = actual(
            state,
            symbol,
            Rc::new(move |_| {
                seen.set(seen.get() + 1);
                Ok(())
            }),
        );
        match result {
            Ok(unsubscribe) => {
                unsubscribe().unwrap();
                assert_eq!(
                    callbacks.get(),
                    0,
                    "no book before a snapshot or connection"
                );
                let keys = fixture
                    .socket
                    .base()
                    .message_handlers
                    .borrow()
                    .keys()
                    .cloned()
                    .collect::<Vec<_>>();
                json!({"unsubscribed":true,"callbacks":callbacks.get(),"handlerKeys":keys})
            }
            Err(error) => json!({"name":error.name,"message":error.message}),
        }
    })
}

struct LocalOrigin {
    inner: Arc<dyn safe_cex_transport::RestAdapter>,
    origin: String,
    seen: Arc<std::sync::Mutex<Vec<String>>>,
}
impl safe_cex_transport::RestAdapter for LocalOrigin {
    fn execute(
        &self,
        mut request: safe_cex_transport::RestRequest,
    ) -> safe_cex_transport::RestFuture<'_, safe_cex_transport::RestResponse> {
        self.seen.lock().unwrap().push(request.url.clone());
        assert!(
            request.url.starts_with('/'),
            "relative native snapshot endpoint"
        );
        assert!(request.base_url.is_some(), "native snapshot base URL");
        request.base_url = Some(self.origin.clone());
        self.inner.execute(request)
    }
}

pub fn snapshot(
    symbol: String,
    limit: f64,
    response: String,
    actual: impl FnOnce(
        &PublicSocket,
        crate::public_listeners::Venue,
        &str,
        f64,
    )
        -> Result<crate::public_listeners::Snapshot, crate::request_weight::Failure>,
) -> Value {
    use std::io::{Read, Write};
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let origin = format!("http://{}", listener.local_addr().unwrap());
    listener.set_nonblocking(true).unwrap();
    let server = std::thread::spawn(move || {
        let deadline = std::time::Instant::now() + std::time::Duration::from_secs(5);
        let mut stream = loop {
            match listener.accept() {
                Ok((stream, _)) => break stream,
                Err(error) if error.kind() == std::io::ErrorKind::WouldBlock => {
                    assert!(
                        std::time::Instant::now() < deadline,
                        "snapshot HTTP was not sent"
                    );
                    std::thread::sleep(std::time::Duration::from_millis(5));
                }
                Err(error) => panic!("snapshot accept failed: {error}"),
            }
        };
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .unwrap();
        let mut request = Vec::new();
        while !request.windows(4).any(|part| part == b"\r\n\r\n") {
            let mut block = [0; 1024];
            let count = stream.read(&mut block).unwrap();
            assert!(
                count > 0 && request.len() < 65536,
                "incomplete snapshot request"
            );
            request.extend_from_slice(&block[..count]);
        }
        let body = response;
        write!(stream, "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}", body.len(), body).unwrap();
        String::from_utf8(request)
            .unwrap()
            .lines()
            .next()
            .unwrap()
            .to_owned()
    });
    let seen = Arc::new(std::sync::Mutex::new(Vec::new()));
    let http = Arc::new(LocalOrigin {
        inner: Arc::new(safe_cex_transport::native::reqwest_adapter().unwrap()),
        origin,
        seen: seen.clone(),
    });
    let result = run_with_http(PublicKind::FuturesPublic, http, |fixture| async move {
        let data = actual(
            &fixture.socket,
            crate::public_listeners::Venue::Futures,
            &symbol,
            limit,
        )
        .unwrap()
        .await
        .unwrap();
        let id = data.read("lastUpdateId").unwrap().number().unwrap();
        let bid = data.read("bids").unwrap().read("0").unwrap();
        let price = bid.read("0").unwrap().text().unwrap();
        let amount = bid.read("1").unwrap().number().unwrap();
        json!({"idBits":format!("{:016x}",id.to_bits()),"bidPrice":price,
            "amountBits":format!("{:016x}",amount.to_bits())})
    });
    let request = server.join().unwrap();
    assert!(
        request.starts_with("GET /fapi/v1/depth?"),
        "actual source snapshot endpoint: {request}"
    );
    assert_eq!(seen.lock().unwrap().len(), 1, "one actual HTTP execution");
    result
}
