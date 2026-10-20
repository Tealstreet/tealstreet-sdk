use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_blofin::{Blofin, Options};
pub const INSTRUMENT: &str = r#"{"code":"0","data":[{"instId":"BTC-USDT","instType":"SWAP","contractType":"linear","baseCurrency":"BTC","quoteCurrency":"USDT","state":"live","contractValue":"0.001","minSize":"0.1","lotSize":"0.1","tickSize":"0.1","maxLimitSize":"1000","maxMarketSize":"500","maxLeverage":"150"}]}"#;
pub const POSITION: &str = r#"{"instId":"BTC-USDT","instType":"SWAP","positionId":"395624","positionSide":"net","marginMode":"cross","positions":"4","averagePrice":"59965.8","markPrice":"60000","maintenanceMargin":"0.72","unrealizedPnl":"0.5","leverage":"20","liquidationPrice":"26914.9"}"#;
pub struct Harness {
    pub e: Engine<Blofin>,
    pub pending: Vec<HttpRequest>,
    pub actions: Vec<Action>,
    pub private: SocketId,
    pub tick: TimerId,
}
impl Harness {
    pub fn new() -> Self {
        Self::new_at(0.0)
    }
    pub fn new_at(now: f64) -> Self {
        Self::with_broker_attribution_at(false, now)
    }
    #[allow(dead_code)] // Used by the trading suite; other suites share this module.
    pub fn with_broker_attribution(broker_attribution: bool) -> Self {
        Self::with_broker_attribution_at(broker_attribution, 0.0)
    }
    fn with_broker_attribution_at(broker_attribution: bool, now: f64) -> Self {
        let mut e = Engine::new(Blofin::new(Options {
            broker_attribution,
            key: Some("key".into()),
            secret: Some("secret".into()),
            passphrase: Some("pass".into()),
            ..Options::default()
        }));
        e.start(now);
        let mut h = Self {
            e,
            pending: vec![],
            actions: vec![],
            private: SocketId(0),
            tick: TimerId(0),
        };
        h.collect();
        let instruments = h.take("instruments");
        h.respond_at(instruments, INSTRUMENT, now);
        let tickers = h.take("tickers");
        h.respond_at(tickers,r#"{"code":"0","data":[{"instId":"BTC-USDT","last":"60000","open24h":"59000","bidPrice":"59999","askPrice":"60001","volCurrency24h":"1","vol24h":"60000"}]}"#,now);
        h.private = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, url, .. } if url.ends_with("/private") => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.tick = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } if *at_ms == now + 10_000.0 => Some(*timer),
                _ => None,
            })
            .unwrap();
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
            .unwrap_or_else(|| panic!("missing {path}; pending {:?}", self.pending));
        self.pending.remove(i)
    }
    pub fn reply(&mut self, path: &str, body: &str) {
        let r = self.take(path);
        self.respond(r, body);
    }
    pub fn respond(&mut self, r: HttpRequest, body: &str) {
        self.respond_at(r, body, 0.0);
    }
    fn respond_at(&mut self, r: HttpRequest, body: &str, now: f64) {
        self.e.handle(
            now,
            Event::HttpResponse {
                request: r.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    pub fn modes(&mut self) {
        self.reply(
            "margin-mode",
            r#"{"code":"0","data":{"marginMode":"cross"}}"#,
        );
        self.reply(
            "position-mode",
            r#"{"code":"0","data":{"positionMode":"net_mode"}}"#,
        );
    }
    pub fn seed(&mut self) {
        self.modes();
        self.reply(
            "account/positions",
            &format!("{{\"code\":\"0\",\"data\":[{POSITION}]}}"),
        );
        self.reply(
            "balances",
            r#"{"code":"0","data":[{"currency":"USDT","available":"1000","frozen":"10"}]}"#,
        );
        for path in [
            "orders-pending",
            "orders-tpsl-pending",
            "orders-algo-pending",
        ] {
            self.reply(path, r#"{"code":"0","data":[]}"#);
        }
    }
    pub fn ws(&mut self, text: &str) {
        self.e.handle(
            1.0,
            Event::WsMessage {
                socket: self.private,
                text,
            },
        );
        self.collect();
    }
}
