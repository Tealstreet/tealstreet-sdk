//! The native loop driven by recorded traffic: responses arrive in chunks,
//! sockets open, frames and commands flow, and the engine ends in the state
//! a direct replay of the same traffic produces.
#![cfg(not(target_arch = "wasm32"))]
#[path = "support/prepared.rs"]
mod prepared;

use safe_cex_engine::wire::{book_top, section, sections};
use safe_cex_engine::{Command, HttpRequest, SocketId};
use safe_cex_native::{BoundHttp, Events, Host, HostEvent, HttpPrepareFuture, Transport};
use safe_cex_venue_binance::{BinanceFutures, Options};
use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::mpsc;

const EXCHANGE_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"120"}]}]}"#;

fn response(url: &str) -> &'static str {
    match url.split('?').next().unwrap().rsplit('/').next().unwrap() {
        "exchangeInfo" => EXCHANGE_INFO,
        "24hr" => {
            r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]"#
        }
        "bookTicker" => r#"[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]"#,
        "premiumIndex" => {
            r#"[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]"#
        }
        "depth" => r#"{"lastUpdateId":100,"bids":[["99.0","5"]],"asks":[["101.0","5"]]}"#,
        other => panic!("unexpected request {other}"),
    }
}

/// Serves canned responses in three chunks and remembers each socket.
#[derive(Clone, Default)]
struct Tape {
    sockets: Arc<Mutex<HashMap<String, (SocketId, Events)>>>,
    prepared: Arc<AtomicUsize>,
    dispatched: Arc<AtomicUsize>,
}

impl Transport for Tape {
    fn prepare_http(
        &self,
        request: HttpRequest,
        intent: safe_cex_engine::request_context::RoutingIntent,
    ) -> HttpPrepareFuture {
        assert_eq!(
            intent,
            safe_cex_engine::request_context::RoutingIntent::Public
        );
        let boundary = prepared::Boundary::new(&request);
        let tape = self.clone();
        self.prepared.fetch_add(1, Ordering::SeqCst);
        Box::pin(async move {
            let boundary = boundary?;
            let deadline = boundary.deadline;
            Ok(boundary.bind_with(move |packet| {
                Ok(Box::new(TapeBound {
                    tape,
                    packet,
                    deadline,
                }))
            }))
        })
    }
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        self.sockets.lock().unwrap().insert(url, (socket, events));
        mpsc::unbounded_channel().0
    }

    fn request(&self, request: HttpRequest, events: Events) {
        let body = response(&request.url).as_bytes();
        let id = request.id;
        let _ = events.send(HostEvent::HttpStart {
            request: id,
            status: 200,
            headers: String::new(),
        });
        for chunk in body.chunks(body.len() / 3 + 1) {
            let _ = events.send(HostEvent::HttpChunk(id, chunk.to_vec()));
        }
        let _ = events.send(HostEvent::HttpEnd(id));
    }
}

struct TapeBound {
    tape: Tape,
    packet: HttpRequest,
    deadline: tokio::time::Instant,
}
impl std::fmt::Debug for TapeBound {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("RecordedBoundHttp")
    }
}
impl BoundHttp for TapeBound {
    fn ready(&self) -> bool {
        tokio::time::Instant::now() < self.deadline
    }
    fn dispatch(self: Box<Self>, events: Events) -> tokio::task::AbortHandle {
        assert!(self.ready(), "recorded handoff exceeded original deadline");
        self.tape.dispatched.fetch_add(1, Ordering::SeqCst);
        self.tape.request(self.packet, events);
        // The recorded emitter above is synchronous and has already finished.
        tokio::spawn(async {}).abort_handle()
    }
}

#[tokio::test]
async fn the_native_loop_streams_startup_and_publishes_books() {
    let tape = Tape::default();
    let engine = safe_cex_engine::test_support::seeded_engine(BinanceFutures::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    let (commands, inbox) = mpsc::unbounded_channel();
    let frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let collected = frames.clone();
    let mut host = tokio::spawn(Host::new(engine, tape.clone()).run(
        inbox,
        Duration::from_millis(10),
        move |frame| collected.lock().unwrap().push(frame.to_vec()),
        |_| {},
    ));

    let public = tokio::time::timeout(Duration::from_secs(3), async {
        loop {
            if let Some(socket) = tape
                .sockets
                .lock()
                .unwrap()
                .get("wss://fstream.binance.com/public/ws")
                .cloned()
            {
                break socket;
            }
            tokio::time::sleep(Duration::from_millis(1)).await;
        }
    })
    .await;
    let public = match public {
        Ok(public) => public,
        Err(_) => {
            host.abort();
            let _ = host.await;
            panic!("native recorded startup did not open its public socket");
        }
    };
    commands
        .send(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
        .unwrap();
    tokio::time::sleep(Duration::from_millis(5)).await;
    let (socket, events) = public;
    for (first, last, bid) in [(90, 105, "98.5"), (106, 110, "98.7")] {
        let text = format!(
            r#"{{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":{first},"u":{last},"pu":{},"b":[["{bid}","1"]],"a":[]}}"#,
            first - 1
        );
        events.send(HostEvent::WsMessage(socket, text)).unwrap();
        tokio::time::sleep(Duration::from_millis(5)).await;
    }
    tokio::time::sleep(Duration::from_millis(30)).await;
    drop(commands);
    let engine = match tokio::time::timeout(Duration::from_secs(3), &mut host).await {
        Ok(engine) => engine.unwrap(),
        Err(_) => {
            host.abort();
            let _ = host.await;
            panic!("native recorded host did not stop");
        }
    };
    assert!(tape.prepared.load(Ordering::SeqCst) > 0);
    assert!(tape.dispatched.load(Ordering::SeqCst) > 0);
    assert!(tape.dispatched.load(Ordering::SeqCst) <= tape.prepared.load(Ordering::SeqCst));

    let book = engine.book("BTCUSDT").expect("synchronized book");
    assert_eq!(book.best_bid(), Some(99.0));
    assert_eq!(book.bids().len(), 3, "snapshot plus both diffs");
    let frames = frames.lock().unwrap();
    let kinds: Vec<u16> = frames
        .iter()
        .flat_map(|f| sections(f).map(|(k, _)| k))
        .collect();
    assert!(kinds.contains(&section::MARKETS) && kinds.contains(&section::TICKERS));
    let top = frames
        .iter()
        .flat_map(|f| {
            sections(f)
                .filter(|(k, _)| *k == section::BOOK)
                .map(|(_, p)| book_top(p))
        })
        .last()
        .flatten();
    assert_eq!(top, Some((0, 99.0, 101.0)));
}
