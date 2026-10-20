#![allow(dead_code)]
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, Response, SocketChannel, SocketId, TimerId,
};
use safe_cex_venue_paradex::{Options, Paradex};
pub const MARKETS: &str = r#"{"results":[{"symbol":"BTC-USD-PERP","base_currency":"BTC","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.00001","price_tick_size":"0.1","min_notional":"10","max_order_size":"100","delta1_cross_margin_params":{"imf_base":"0.02"}},{"symbol":"ETH-USD-PERP","base_currency":"ETH","quote_currency":"USD","asset_kind":"PERP","order_size_increment":"0.001","price_tick_size":"0.01","min_notional":"10","max_order_size":"1000","initial_margin_fraction":"0.05"},{"symbol":"BTC-USD-250101-100000-C","asset_kind":"OPTION"},{"symbol":"USDC-USD-SPOT","asset_kind":"SPOT"},{"symbol":"OLD-USD-PERP","asset_kind":"PERP","status":"INACTIVE"}]}"#;
pub const TICKERS: &str = r#"{"results":[{"symbol":"BTC-USD-PERP","bid":"99","ask":"101","last_traded_price":"100","mark_price":"100.5","underlying_price":"100.2","price_change_rate_24h":"0.02","funding_rate":"0.0001","volume_24h":"200","open_interest":"50"},{"symbol":"ETH-USD-PERP","bid":"9","ask":"11","last_traded_price":"10","mark_price":"10.5","volume_24h":"20"},{"symbol":"BTC-USD-250101-100000-C","last_traded_price":"4"}]}"#;
pub struct Harness {
    pub e: Engine<Paradex>,
    pub pending: Vec<HttpRequest>,
    pub actions: Vec<Action>,
    pub public: SocketId,
    pub now: f64,
}
impl Harness {
    pub fn empty(options: Options) -> Self {
        let mut e = Engine::new(Paradex::new(options));
        e.start(1_700_000_000_000.);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            public: SocketId(0),
            now: 1_700_000_000_000.,
        };
        h.collect();
        h
    }
    pub fn new() -> Self {
        let mut h = Self::empty(Options {
            public_data_only: true,
            ..Options::default()
        });
        h.reply("/markets", MARKETS);
        h.reply("summary", TICKERS);
        h.public = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Public),
                    ..
                } => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.e.handle(h.now, Event::WsOpened { socket: h.public });
        h.collect();
        h
    }
    pub fn collect(&mut self) {
        for a in self.e.drain_actions() {
            if let Action::Http(r) = a {
                self.pending.push(r);
            } else {
                self.actions.push(a);
            }
        }
    }
    pub fn take(&mut self, path: &str) -> HttpRequest {
        let i = self
            .pending
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.pending));
        self.pending.remove(i)
    }
    pub fn respond(&mut self, r: HttpRequest, status: u16, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    pub fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, 200, body);
    }
    pub fn ws(&mut self, text: &str) {
        self.e.handle(
            self.now,
            Event::WsMessage {
                socket: self.public,
                text,
            },
        );
        self.collect();
    }
    pub fn command(&mut self, c: Command) {
        self.e.handle(self.now, Event::Command(c));
        self.collect();
    }
    pub fn timer_at(&self, at: f64) -> TimerId {
        self.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
                _ => None,
            })
            .unwrap()
    }
    pub fn timer(&mut self, at: f64) {
        let timer = self.timer_at(at);
        self.now = at;
        self.e.handle(self.now, Event::TimerFired { timer });
        self.collect();
    }
    pub fn sent(&self) -> Vec<&str> {
        self.actions
            .iter()
            .filter_map(|a| match a {
                Action::WsSend { text, .. } => Some(text.as_str()),
                _ => None,
            })
            .collect()
    }
    pub fn result(&self, id: u32) -> Result<&str, &str> {
        self.actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::Notice(Notice::CommandDone { command, result }) if *command == id => Some(
                    result
                        .as_ref()
                        .map(String::as_str)
                        .map_err(|error| error.message.as_str()),
                ),
                _ => None,
            })
            .unwrap()
    }
}
