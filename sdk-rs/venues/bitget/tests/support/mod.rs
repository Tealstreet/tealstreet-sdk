#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, RequestId, Response, SocketContext,
    SocketId, TimerId,
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
    pub socket_contexts: Vec<(SocketId, String, Option<SocketContext>)>,
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
            socket_contexts: vec![],
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
                Action::SignPayload { .. } => {
                    panic!("delegated signing is unsupported in the bitget test harness")
                }
                Action::Http(r) => self.requests.push(r),
                Action::WsConnect {
                    socket,
                    url,
                    context,
                    ..
                }
                | Action::WsConnectWithHeaders {
                    socket,
                    url,
                    context,
                    ..
                } => {
                    self.socket_contexts.push((socket, url.clone(), context));
                    self.sockets.push((socket, url))
                }
                Action::WsSend { socket, text } => self.sent.push((socket, text)),
                Action::WsClose { socket } => self.closed.push(socket),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::CommandDone { command, result }) => {
                    self.done.push((command, result))
                }
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.done.push((command, Err(error.legacy_text())))
                }
                Action::Notice(Notice::Log { message, .. }) => self.logs.push(message),
                Action::Notice(Notice::Deactivate) => {
                    panic!("unexpected Bitget account deactivation")
                }
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

pub const POSITION: &str = r#"{"code":"00000","data":[{"symbol":"BTCUSDT","holdSide":"long","total":"1","openPriceAvg":"90","markPrice":"100","leverage":"20","unrealizedPL":"10","liquidationPrice":"50","marginSize":"50","marginMode":"crossed","posMode":"one_way_mode","uTime":"100"}]}"#;
pub const ACCOUNT: &str = r#"{"code":"00000","data":[{"marginCoin":"USDT","unionTotalMargin":"1010","unionAvailable":"900","unrealizedPL":"10"}]}"#;
pub const OPEN_ORDER: &str = r#"{"code":"00000","data":{"entrustedList":[{"orderId":"1","symbol":"BTCUSDT","size":"2","baseVolume":"0","price":"90","side":"buy","orderType":"limit","status":"live","clientOid":"caller"}],"endId":""}}"#;

impl Harness {
    pub fn private_started() -> Self {
        let mut h = Self::started(Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Options::default()
        });
        h.load_public();
        h
    }
    pub fn classic() -> Self {
        let mut h = Self::private_started();
        h.respond(
            "/account/settings",
            200,
            r#"{"code":"40084","msg":"Classic Account mode","data":null}"#,
        );
        h.respond("/all-position", 200, POSITION);
        h.respond("/account/accounts", 200, ACCOUNT);
        h.orders(OPEN_ORDER);
        h.login();
        h
    }
    pub fn orders(&mut self, normal: &str) {
        self.respond("/orders-pending", 200, normal);
        self.respond(
            "planType=profit_loss",
            200,
            r#"{"code":"00000","data":{"entrustedList":[]}}"#,
        );
        self.respond("planType=track_plan", 200, r#"{"code":"00000","data":[]}"#);
    }
    pub fn login(&mut self) {
        let socket = self
            .sockets
            .iter()
            .find(|(_, u)| u.ends_with("/private"))
            .unwrap()
            .0;
        self.event(Event::WsOpened { socket });
        self.private_message(r#"{"event":"login","code":0,"msg":"","connId":"extra-field"}"#);
    }
    pub fn private_message(&mut self, text: &str) {
        let socket = self
            .sockets
            .iter()
            .rev()
            .find(|(_, u)| u.ends_with("/private"))
            .unwrap()
            .0;
        self.event(Event::WsMessage { socket, text });
    }
    pub fn timer_after(&mut self, delay: f64) {
        let (timer, at) = self
            .timers
            .iter()
            .copied()
            .find(|(_, at)| (*at - self.now - delay).abs() < 0.01)
            .unwrap_or_else(|| panic!("no timer after {delay} at {}: {:?}", self.now, self.timers));
        self.timers.retain(|(t, _)| *t != timer);
        self.now = at;
        self.event(Event::TimerFired { timer });
    }
}
