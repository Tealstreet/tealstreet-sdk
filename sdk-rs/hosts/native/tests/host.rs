//! The native loop driven by recorded traffic: responses arrive in chunks,
//! sockets open, frames and commands flow, and the engine ends in the state
//! a direct replay of the same traffic produces.
#![cfg(not(target_arch = "wasm32"))]

use safe_cex_engine::wire::{book_top, section, sections};
use safe_cex_engine::{Command, Engine, HttpRequest, SocketId};
use safe_cex_native::{Events, Host, HostEvent, Transport};
use safe_cex_venue_binance::{BinanceFutures, Options};
use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;
use tokio::sync::{Notify, mpsc};

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
    book_subscribed: Arc<Notify>,
    depth_responded: Arc<Notify>,
}

impl Transport for Tape {
    fn connect(
        &self,
        socket: SocketId,
        url: String,
        events: Events,
    ) -> mpsc::UnboundedSender<String> {
        let _ = events.send(HostEvent::WsOpened(socket));
        self.sockets.lock().unwrap().insert(url, (socket, events));
        let (sender, mut receiver) = mpsc::unbounded_channel::<String>();
        let book_subscribed = self.book_subscribed.clone();
        tokio::spawn(async move {
            while let Some(text) = receiver.recv().await {
                if text.contains(r#""SUBSCRIBE""#) && text.contains(r#""btcusdt@depth""#) {
                    book_subscribed.notify_one();
                }
            }
        });
        sender
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
        if request.url.contains("/depth?") {
            self.depth_responded.notify_one();
        }
    }
}

#[tokio::test]
async fn the_native_loop_streams_startup_and_publishes_books() {
    let tape = Tape::default();
    let engine = Engine::new(BinanceFutures::new(Options {
        public_data_only: true,
        ..Options::default()
    }));
    let (commands, inbox) = mpsc::unbounded_channel();
    let frames = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
    let collected = frames.clone();
    let host = tokio::spawn(Host::new(engine, tape.clone()).run(
        inbox,
        Duration::from_millis(10),
        move |frame| collected.lock().unwrap().push(frame.to_vec()),
        |_| {},
    ));

    commands
        .send(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        })
        .unwrap();
    // Observe the command's actual transport action before sending its feed.
    tape.book_subscribed.notified().await;
    let (socket, events) = tape
        .sockets
        .lock()
        .unwrap()
        .get("wss://fstream.binance.com/public/ws")
        .cloned()
        .unwrap();
    for (index, (first, last, bid)) in [(90, 105, "98.5"), (106, 110, "98.7")]
        .into_iter()
        .enumerate()
    {
        let text = format!(
            r#"{{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":{first},"u":{last},"pu":{},"b":[["{bid}","1"]],"a":[]}}"#,
            first - 1
        );
        events.send(HostEvent::WsMessage(socket, text)).unwrap();
        if index == 0 {
            // The first diff requests a snapshot; enqueue it before the next
            // diff, without depending on how quickly the host task runs.
            tape.depth_responded.notified().await;
        }
    }
    // Closing commands drains ready traffic and publishes the final frame.
    drop(commands);
    let engine = host.await.unwrap();

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
