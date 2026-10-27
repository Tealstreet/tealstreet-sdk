//! Engine-managed socket lifecycle for Bitunix's futures and private feeds:
//! frames are admitted only on the open, current, unexpired epoch, and each
//! open runs its subscriptions and login once.
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, TimerId};
mod support;
use safe_cex_venue_bitunix::BitunixFutures;

const DEPTH: &str = r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["96000","2"]],"asks":[["96001","3"]]}}"#;
const LATER_DEPTH: &str = r#"{"channel":"market_btcusdt_depthop_0.1","data":{"bids":[["95000","2"]],"asks":[["95001","3"]]}}"#;
fn order(id: &str) -> String {
    format!(
        r#"{{"ch":"order","data":{{"event":"CREATE","symbol":"BTCUSDT","orderId":"{id}","side":"BUY","type":"LIMIT","qty":"2","price":"90000","dealAmount":"0"}}}}"#
    )
}
/// Futures idle bound: the legacy supervisor's 45 s.
const IDLE_MS: f64 = 45_000.0;

struct H {
    engine: Engine<BitunixFutures>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String)>,
    sent: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
}

impl H {
    /// A keyed account with markets loaded; no socket is open yet.
    fn start() -> Self {
        let mut h = Self {
            engine: support::engine(support::Options {
                key: Some("replay-key".into()),
                secret: Some("replay-material".into()),
                ..support::Options::default()
            }),
            now: 1_700_000_000_000.0,
            requests: vec![],
            sockets: vec![],
            sent: vec![],
            timers: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h.reply("trading_pairs", support::PAIRS);
        if h.requests.iter().any(|r| r.url.contains("/tickers")) {
            h.reply("/tickers", support::TICKERS);
        }
        h
    }
    /// Startup account reads answered, the private socket not yet open.
    fn account_unopened() -> Self {
        let mut h = Self::start();
        for (part, body) in [
            ("get_single_account", support::ACCOUNT),
            ("get_pending_positions", support::POSITIONS),
            ("get_pending_orders", support::ORDERS),
            ("/tpsl/", support::TPSL),
        ] {
            if h.requests.iter().any(|r| r.url.contains(part)) {
                h.reply(part, body);
            }
        }
        h
    }
    /// As `account_unopened`, with the private socket open.
    fn account() -> Self {
        let mut h = Self::account_unopened();
        let socket = h.private();
        h.open(socket);
        h
    }
    fn futures(&self) -> SocketId {
        self.socket("ws-futures")
    }
    fn private(&self) -> SocketId {
        self.socket("/private/")
    }
    fn socket(&self, part: &str) -> SocketId {
        self.sockets
            .iter()
            .rev()
            .find(|(_, url)| url.contains(part))
            .unwrap_or_else(|| panic!("no {part} socket: {:?}", self.sockets))
            .0
    }
    fn sent_on(&self, socket: SocketId, text: &str) -> usize {
        self.sent
            .iter()
            .filter(|(s, t)| *s == socket && t.contains(text))
            .count()
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                _ => {}
            }
        }
    }
    fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }
    fn reply(&mut self, part: &str, body: &str) {
        let position = self
            .requests
            .iter()
            .position(|r| r.url.contains(part))
            .unwrap_or_else(|| panic!("missing {part}"));
        let request = self.requests.remove(position);
        self.event(Event::HttpResponse {
            request: request.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn open(&mut self, socket: SocketId) {
        self.event(Event::WsOpened { socket });
    }
    fn message(&mut self, socket: SocketId, text: &str) {
        self.event(Event::WsMessage { socket, text });
    }
    fn subscribe_book(&mut self) {
        self.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
    }
    fn best_bid(&self) -> Option<f64> {
        self.engine.book("BTCUSDT").and_then(|book| book.best_bid())
    }
}

// REDs: these fail on the legacy supervisor, which admits a frame on a
// socket whose idle deadline is due until its timer is delivered.

#[test]
fn futures_frame_at_idle_equality_is_not_published() {
    let mut h = H::start();
    h.subscribe_book();
    let socket = h.futures();
    h.open(socket);
    h.message(socket, DEPTH);
    assert_eq!(h.best_bid(), Some(96_000.0), "a live epoch publishes");
    // The idle deadline is due but its timer has not been delivered yet.
    h.now += IDLE_MS;
    h.message(socket, LATER_DEPTH);
    assert_ne!(h.best_bid(), Some(95_000.0));
}

#[test]
fn private_frame_at_idle_equality_after_the_first_pong_is_not_reduced() {
    let mut h = H::account();
    let socket = h.private();
    h.message(socket, r#"{"op":"pong"}"#);
    h.message(socket, &order("live"));
    assert!(
        h.engine.store().order("live").is_some(),
        "a live epoch reduces"
    );
    h.now += IDLE_MS;
    h.message(socket, &order("late"));
    assert!(h.engine.store().order("late").is_none());
}

// Controls: green before and after adoption. Legacy Bitunix already fences
// duplicate opens and frames before open; adoption must keep that.

#[test]
fn futures_duplicate_open_does_not_resubscribe() {
    let mut h = H::start();
    h.subscribe_book();
    let socket = h.futures();
    h.open(socket);
    let subscribes = h.sent_on(socket, "depthop");
    assert_eq!(subscribes, 1);
    h.open(socket);
    assert_eq!(h.sent_on(socket, "depthop"), subscribes);
}

#[test]
fn futures_frame_before_open_is_not_published() {
    let mut h = H::start();
    h.subscribe_book();
    let socket = h.futures();
    h.message(socket, DEPTH);
    assert_eq!(h.best_bid(), None);
    h.open(socket);
    h.message(socket, DEPTH);
    assert_eq!(h.best_bid(), Some(96_000.0), "an open epoch publishes");
}

#[test]
fn private_duplicate_open_does_not_log_in_twice() {
    let mut h = H::start();
    let socket = h.private();
    h.open(socket);
    assert_eq!(h.sent_on(socket, r#""op":"login""#), 1);
    h.open(socket);
    assert_eq!(h.sent_on(socket, r#""op":"login""#), 1);
}

#[test]
fn private_frame_before_open_is_not_reduced() {
    let mut h = H::account_unopened();
    let socket = h.private();
    h.message(socket, &order("early"));
    assert!(h.engine.store().order("early").is_none());
    h.open(socket);
    h.message(socket, &order("after"));
    assert!(
        h.engine.store().order("after").is_some(),
        "an open epoch reduces"
    );
}

#[test]
fn futures_open_subscribes_each_interest_once() {
    let mut h = H::start();
    h.subscribe_book();
    let socket = h.futures();
    h.open(socket);
    assert_eq!(h.sent_on(socket, "market_btcusdt_depthop_0.1"), 1);
}

#[test]
fn private_open_pings_then_logs_in() {
    let mut h = H::start();
    let socket = h.private();
    h.open(socket);
    let frames: Vec<_> = h
        .sent
        .iter()
        .filter(|(s, _)| *s == socket)
        .map(|(_, t)| t.as_str())
        .collect();
    let ping = frames.iter().position(|t| t.contains(r#""op":"ping""#));
    let login = frames.iter().position(|t| t.contains(r#""op":"login""#));
    assert!(ping.is_some() && login.is_some(), "{frames:?}");
    assert!(ping < login, "the ping precedes the login: {frames:?}");
}
