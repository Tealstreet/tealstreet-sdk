use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, TimerId, entropy::Entropy};
use safe_cex_venue_bitunix::{BitunixFutures, Options};
use sha2::{Digest, Sha256};
use std::{collections::{HashMap, VecDeque}, io::{BufRead, Write}, time::{Duration, Instant}};

type E = Engine<BitunixFutures>;
#[derive(Debug)]
struct Record {
    time: f64,
    kind: String,
    url: String,
    slot: usize,
    text: String,
    body: String,
    method: String,
    status: u16,
}
struct Driver {
    engine: E,
    requests: VecDeque<HttpRequest>,
    connects: VecDeque<(SocketId, String)>,
    sockets: Vec<Option<SocketId>>,
    timers: Vec<(u64,f64)>,
    timer_fired: usize,
    frames: usize,
    sends: usize,
    wire_bytes: usize,
}
impl Driver {
    fn advance(&mut self, target: f64) {
        for _ in 0..100_000 {
            let Some(&(id,at)) = self.timers.first().filter(|(_,at)| *at <= target) else { return; };
            self.timers.remove(0); self.timer_fired += 1;
            self.engine.handle(at,Event::TimerFired { timer: TimerId(id) });
            self.flush();
        }
        panic!("unbounded profile timer loop");
    }
    fn flush(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(request) => self.requests.push_back(request),
                Action::WsConnect { socket, url, .. } => self.connects.push_back((socket, url)),
                Action::WsSend { text, .. } => { self.sends += 1; self.wire_bytes += text.len(); },
                Action::SetTimer { timer, at_ms } => {
                    assert!(at_ms.is_finite()); self.timers.retain(|(id,_)| *id != timer.0);
                    self.timers.push((timer.0,at_ms)); self.timers.sort_by(|(a,at),(b,bt)|at.total_cmp(bt).then(a.cmp(b)));
                },
                Action::ClearTimer { timer } => { self.timers.retain(|(id,_)| *id != timer.0); },
                Action::Notice(_) => {},
                other => panic!("unexpected public profile action: {other:?}"),
            }
        }
    }
}
fn entropy() -> Entropy {
    fn fill() -> safe_cex_engine::entropy::Fill {
        let mut state = 41u64;
        Box::new(move |bytes| { for byte in bytes { state ^= state << 13; state ^= state >> 7; state ^= state << 17; *byte = state as u8; } Ok(()) })
    }
    Entropy::new(fill(), fill())
}
#[inline(never)]
fn replay(records: &[Record]) -> Driver {
    let engine = Engine::new(BitunixFutures::new(Options { public_data_only: true, ..Options::default() }), Some(entropy())).unwrap();
    let mut d = Driver { engine, requests: VecDeque::new(), connects: VecDeque::new(), sockets: vec![None; 7], timers: Vec::new(), timer_fired: 0, frames: 0, sends: 0, wire_bytes: 0 };
    d.engine.start(0.0); d.flush();
    for symbol in ["BTCUSDT", "ETHUSDT"] {
        d.engine.handle(0.0, Event::Command(Command::SubscribeOrderBook { symbol: symbol.into() })); d.flush();
    }
    let mut last_frame = -1000.0;
    for record in records {
        d.advance(record.time);
        match record.kind.as_str() {
            "http" => {
                let position = d.requests.iter().position(|request| request.url == record.url && request.method.as_str() == record.method).expect("unmatched tape HTTP");
                let request = d.requests.remove(position).unwrap();
                d.engine.handle(record.time, Event::HttpResponse { request: request.id, response: Response { status: record.status, headers: "", body: &record.body } });
                d.flush();
            },
            "connect" => {
                let (socket, url) = d.connects.pop_front().expect("missing physical connect");
                assert_eq!(url, record.url); assert!(d.sockets[record.slot].replace(socket).is_none());
            },
            "open" => {
                let socket = d.sockets[record.slot].unwrap();
                d.engine.handle(record.time, Event::WsOpened { socket }); d.flush();
            },
            "ws" => {
                let socket = d.sockets[record.slot].unwrap();
                d.engine.handle(record.time, Event::WsMessage { socket, text: &record.text }); d.flush(); d.frames += 1;
            },
            other => panic!("unexpected record {other}"),
        }
        if record.time - last_frame >= 250.0 {
            if let Some(frame) = d.engine.take_frame(record.time) { std::hint::black_box(frame); }
            last_frame = record.time;
        }
    }
    assert_eq!(d.frames, 28687); assert!(d.requests.is_empty() && d.connects.is_empty());
    assert!(d.sockets.iter().all(Option::is_some));
    assert!(d.engine.book("BTCUSDT").is_some() && d.engine.book("ETHUSDT").is_some());
    d
}
fn signature(d: &Driver) -> (String, usize, usize, usize) {
    let mut markets: Vec<_> = d.engine.store().markets().iter().collect();
    markets.sort_by(|a,b|a.symbol.cmp(&b.symbol));
    let mut tickers: Vec<_> = d.engine.store().tickers().iter().collect();
    tickers.sort_by(|a,b|a.symbol.cmp(&b.symbol));
    let books: Vec<_> = ["BTCUSDT", "ETHUSDT"].into_iter().map(|symbol| {
        let book = d.engine.book(symbol).unwrap();
        serde_json::json!({"symbol":symbol,"bids":book.bids(),"asks":book.asks()})
    }).collect();
    let bytes = serde_json::to_vec(&serde_json::json!({"markets":markets,"tickers":tickers,"books":books})).unwrap();
    (format!("{:x}",Sha256::digest(&bytes)),d.frames,d.sends,d.wire_bytes)
}
fn main() {
    let args: Vec<_> = std::env::args().collect();
    let file = std::fs::File::open(&args[1]).unwrap();
    let seconds: f64 = args.get(2).map(|s|s.parse().unwrap()).unwrap_or(0.0);
    let mut slots = HashMap::new();
    let records: Vec<_> = std::io::BufReader::new(file).lines().map(|line| {
        let v: serde_json::Value = serde_json::from_str(&line.unwrap()).unwrap();
        let slot = if matches!(v["type"].as_str().unwrap(), "connect" | "open" | "ws") {
            let key = (v["url"].as_str().unwrap().to_string(), v["connection"].as_u64().unwrap());
            if v["type"] == "connect" { let n = slots.len(); assert!(slots.insert(key.clone(),n).is_none()); }
            slots[&key]
        } else { 0 };
        Record { time: v["t"].as_f64().unwrap(), kind: v["type"].as_str().unwrap().into(),
            url: v["url"].as_str().unwrap_or("").into(), slot,
            text: v["text"].as_str().unwrap_or("").into(),body: v["body"].as_str().unwrap_or("").into(),
            method: v["method"].as_str().unwrap_or("").into(), status: v["status"].as_u64().unwrap_or(200) as u16 }
    }).collect();
    let expected = signature(&replay(&records));
    for _ in 0..2 { assert_eq!(signature(&replay(&records)), expected); }
    println!("READY pid={} rows={} result={expected:?}",std::process::id(),records.len()); std::io::stdout().flush().unwrap();
    let start = Instant::now(); let mut passes=0u64;
    let last = loop {
        let driver = replay(&records);
        assert_eq!((driver.frames,driver.sends,driver.wire_bytes),(expected.1,expected.2,expected.3));
        std::hint::black_box(&driver); passes += 1;
        if start.elapsed() >= Duration::from_secs_f64(seconds) { break driver; }
    };
    assert_eq!(signature(&last),expected);
    println!("DONE passes={passes} elapsed={:?} result={expected:?}",start.elapsed());
}
