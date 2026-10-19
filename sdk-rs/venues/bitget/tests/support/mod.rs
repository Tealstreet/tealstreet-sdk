#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketId, TimerId,
};
use safe_cex_venue_bitget::{BitgetFutures, Options};

// Bitget Classic contracts/tickers examples, reduced to the fields we consume.
pub const CONTRACTS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","baseCoin":"BTC","quoteCoin":"USDT","symbolStatus":"normal","pricePlace":"1","priceEndStep":"1","sizeMultiplier":"0.001","minTradeNum":"0.001","maxOrderQty":"120","maxLever":"125"}]}"#;
pub const TICKERS: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","bidPr":"99.9","askPr":"100.1","lastPr":"100","indexPrice":"99","changeUtc24h":"0.01","baseVolume":"1000","quoteVolume":"100000","fundingRate":"0.0001"}]}"#;

pub struct Harness {
    pub engine: Engine<BitgetFutures>,
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
    pub fn started(options: Options) -> Self {
        let mut h = Self {
            engine: Engine::new(BitgetFutures::new(options)),
            now: 1_000_000.0,
            requests: vec![],
            sockets: vec![],
            sent: vec![],
            closed: vec![],
            timers: vec![],
            done: vec![],
            logs: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h
    }
    pub fn public() -> Self {
        let mut h = Self::started(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.load_public();
        h
    }
    pub fn load_public(&mut self) {
        self.respond("/contracts", 200, CONTRACTS);
        self.respond("/tickers", 200, TICKERS);
        for (socket, _) in self.sockets.clone() {
            self.event(Event::WsOpened { socket });
        }
    }
    pub fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(r) => self.requests.push(r),
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
    pub fn command(&mut self, command: Command) {
        self.event(Event::Command(command));
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("no {path}: {:?}", self.requests));
        self.requests.remove(i)
    }
    pub fn reply(&mut self, request: RequestId, status: u16, body: &str) {
        self.event(Event::HttpResponse {
            request,
            response: Response {
                status,
                headers: "",
                body,
            },
        });
    }
    pub fn respond(&mut self, path: &str, status: u16, body: &str) {
        let r = self.take(path);
        self.reply(r.id, status, body);
    }
    pub fn message(&mut self, text: &str) {
        self.event(Event::WsMessage {
            socket: self.sockets[0].0,
            text,
        });
    }
    pub fn book(&mut self) {
        self.command(Command::SubscribeOrderBook {
            symbol: "BTCUSDT".into(),
        });
    }
}
