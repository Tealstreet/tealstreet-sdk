//! Drives a Binance engine through scripted responses and frames.
#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_binance::{BinanceFutures, Options};

pub const EXCHANGE_INFO: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","marginAsset":"USDT","contractType":"PERPETUAL","filters":[{"filterType":"PRICE_FILTER","tickSize":"0.10"},{"filterType":"LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"1000"},{"filterType":"MARKET_LOT_SIZE","stepSize":"0.001","minQty":"0.001","maxQty":"120"}]}]}"#;
pub const DAILY: &str = r#"[{"symbol":"BTCUSDT","lastPrice":"100","priceChangePercent":"1","volume":"2","quoteVolume":"3"}]"#;
pub const BOOK_TICKERS: &str = r#"[{"symbol":"BTCUSDT","bidPrice":"99.9","askPrice":"100.1"}]"#;
pub const PREMIUM: &str = r#"[{"symbol":"BTCUSDT","markPrice":"100","indexPrice":"100","lastFundingRate":"0.0001","nextFundingTime":1700000000000}]"#;

pub struct Harness {
    pub engine: Engine<BinanceFutures>,
    pub now: f64,
    pub requests: Vec<HttpRequest>,
    pub sockets: Vec<(SocketId, String)>,
    pub sent: Vec<(SocketId, String)>,
    pub closed: Vec<SocketId>,
    pub timers: Vec<(TimerId, f64)>,
    pub done: Vec<(u32, Result<String, String>)>,
    pub logs: Vec<String>,
}

impl Harness {
    /// A public-data engine with markets and tickers loaded and sockets open.
    pub fn new() -> Self {
        let mut harness = Self::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        harness.load_public();
        harness.event(Event::Command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        }));
        harness
    }

    pub fn started(options: Options) -> Self {
        let mut harness = Self {
            engine: Engine::new(BinanceFutures::new(options)),
            now: 1_000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            sent: Vec::new(),
            closed: Vec::new(),
            timers: Vec::new(),
            done: Vec::new(),
            logs: Vec::new(),
        };
        harness.engine.start(harness.now);
        harness.collect();
        harness
    }

    pub fn load_public(&mut self) {
        self.respond("/fapi/v1/exchangeInfo", 200, EXCHANGE_INFO);
        self.respond("/fapi/v1/ticker/24hr", 200, DAILY);
        self.respond("/fapi/v1/ticker/bookTicker", 200, BOOK_TICKERS);
        self.respond("/fapi/v1/premiumIndex", 200, PREMIUM);
        for path in ["/public/ws", "/market/ws"] {
            let socket = self.socket(path).expect("socket connects after tickers");
            self.event(Event::WsOpened { socket });
        }
    }

    pub fn collect(&mut self) {
        for action in self.engine.drain_actions().collect::<Vec<_>>() {
            match action {
                Action::Http(request) => self.requests.push(request),
                Action::WsConnect { socket, url } => self.sockets.push((socket, url)),
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, result))
                }
                Action::Notice(Notice::Log { message, .. }) => self.logs.push(message),
            }
        }
    }

    pub fn event(&mut self, event: Event<'_>) {
        self.engine.handle(self.now, event);
        self.collect();
    }

    /// The most recent socket whose URL contains `path`.
    pub fn socket(&self, path: &str) -> Option<SocketId> {
        self.sockets
            .iter()
            .rev()
            .find(|(_, url)| url.contains(path))
            .map(|(socket, _)| *socket)
    }

    pub fn url_of(&self, socket: SocketId) -> &str {
        &self.sockets.iter().find(|(s, _)| *s == socket).unwrap().1
    }

    pub fn has_request(&self, path: &str) -> bool {
        self.requests.iter().any(|r| r.url.contains(path))
    }

    pub fn take(&mut self, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("no request for {path}; have {:?}", self.paths()));
        self.requests.remove(index)
    }

    pub fn paths(&self) -> Vec<String> {
        self.requests
            .iter()
            .map(|r| format!("{} {}", r.method.as_str(), r.url))
            .collect()
    }

    pub fn reply(&mut self, request: RequestId, status: u16, headers: &str, body: &str) {
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers,
                body,
            },
        });
    }

    pub fn respond(&mut self, path: &str, status: u16, body: &str) {
        let request = self.take(path).id;
        self.reply(request, status, "", body);
    }

    pub fn message(&mut self, path: &str, text: &str) {
        let socket = self.socket(path).unwrap();
        self.event(Event::WsMessage { socket, text });
    }

    pub fn diff(&mut self, first: u64, last: u64, previous: u64, bid: &str) {
        let text = format!(
            r#"{{"e":"depthUpdate","E":1,"T":1,"s":"BTCUSDT","U":{first},"u":{last},"pu":{previous},"b":[["{bid}","1"]],"a":[]}}"#
        );
        self.message("/public/ws", &text);
    }

    pub fn snapshot_requests(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| r.url.contains("/fapi/v1/depth"))
            .count()
    }

    pub fn best_bid(&self) -> Option<f64> {
        self.engine.book("BTCUSDT").and_then(|book| book.best_bid())
    }

    /// Move the clock forward, firing every timer that comes due.
    pub fn advance(&mut self, ms: f64) {
        let until = self.now + ms;
        loop {
            self.timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some(&(timer, at)) = self.timers.first() else {
                break;
            };
            if at > until {
                break;
            }
            self.timers.remove(0);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
        }
        self.now = until;
    }

    pub fn result(&self, command: u32) -> Option<&Result<String, String>> {
        self.done
            .iter()
            .find(|(c, _)| *c == command)
            .map(|(_, r)| r)
    }
}
