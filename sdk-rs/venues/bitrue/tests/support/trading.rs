//! The scripted trading harness shared by the trading suites.
use super::Options;
use safe_cex_engine::model::*;
use safe_cex_engine::store::MarketSetting;
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Method, Notice, Response, SocketId, TimerId,
};
use safe_cex_venue_bitrue::Bitrue;
pub const CONTRACT: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":0.001,"minOrderVolume":1,"maxMarketVolume":10,"maxLimitVolume":10,"minOrderMoney":5,"status":1,"side":1,"type":"E","multiplierCoin":"BTC"}]"#;
pub enum Trace {
    Progress(u32, Vec<OrderIdentityReceipt>),
    Http(safe_cex_engine::RequestId),
}
pub struct Harness {
    pub e: Engine<Bitrue>,
    pub requests: Vec<HttpRequest>,
    pub notices: Vec<Notice>,
    pub failures: Vec<safe_cex_engine::VenueError>,
    pub trace: Vec<Trace>,
    pub timers: Vec<(TimerId, f64)>,
    pub sockets: Vec<(SocketId, String)>,
    /// The latest event time; managed requests prepare and answer no earlier.
    pub now: f64,
}
impl Harness {
    pub fn new() -> Self {
        Self::with(Options {
            key: Some("replay-key".into()),
            secret: Some("replay-secret".into()),
            ..Options::default()
        })
    }
    pub fn with(options: Options) -> Self {
        let mut e = super::engine(options);
        e.start(0.0);
        let mut h = Self {
            e,
            requests: vec![],
            notices: vec![],
            failures: vec![],
            trace: vec![],
            timers: vec![],
            sockets: vec![],
            now: 100.0,
        };
        h.collect();
        let request = h.take(Method::Get, "/fapi/v1/contracts");
        h.reply(request, CONTRACT);
        h.e.store_mut().update_setting("E-BTC-USDT", |setting| {
            *setting = MarketSetting {
                leverage: 20.0,
                margin_type: Some(MarginType::Cross),
                ..MarketSetting::default()
            }
        });
        h.e.store_mut().set_tickers(vec![Ticker {
            id: "E-BTC-USDT".into(),
            symbol: "E-BTC-USDT".into(),
            last: 2000.0,
            mark: 2000.0,
            ..Ticker::default()
        }]);
        h
    }
    /// Collects released work, then fires any timer armed for this very
    /// instant: venue completions deferred out of a delivery run one tick later.
    pub fn collect(&mut self) {
        loop {
            let armed = self.timers.len();
            self.collect_once();
            let now = self.now;
            let due: Vec<_> = self.timers[armed..]
                .iter()
                .filter(|(_, at)| *at <= now)
                .map(|(timer, _)| *timer)
                .collect();
            if due.is_empty() {
                break;
            }
            self.timers.retain(|(timer, _)| !due.contains(timer));
            for timer in due {
                self.e.handle(now, Event::TimerFired { timer });
            }
        }
    }
    pub fn collect_once(&mut self) {
        for action in
            safe_cex_engine::test_support::prepared_actions(&mut self.e, self.now).into_iter()
        {
            match &action {
                Action::Http(r) => self.trace.push(Trace::Http(r.id)),
                Action::Notice(Notice::CommandProgress { command, receipts }) => {
                    self.trace.push(Trace::Progress(*command, receipts.clone()))
                }
                _ => {}
            }
            match action {
                Action::Http(r) => self.requests.push(r),
                // Failures as the plain text the host renders.
                Action::Notice(Notice::CommandFailed { command, error }) => {
                    self.notices.push(Notice::CommandFailed {
                        command,
                        error: error.clone(),
                    });
                    self.failures.push(error);
                }
                Action::Notice(n) => self.notices.push(n),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                _ => {}
            }
        }
    }
    pub fn take(&mut self, method: Method, path: &str) -> HttpRequest {
        let index = self
            .requests
            .iter()
            .position(|r| r.method == method && r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {method:?} {path}, requests {:?}", self.requests));
        self.requests.remove(index)
    }
    pub fn handle(&mut self, at: f64, event: Event<'_>) {
        self.now = self.now.max(at);
        self.e.handle(at, event);
    }
    pub fn reply(&mut self, request: HttpRequest, body: &str) {
        self.e.handle(
            self.now,
            Event::HttpResponse {
                request: request.id,
                response: Response {
                    status: 200,
                    headers: "",
                    body,
                },
            },
        );
        self.collect();
    }
    pub fn command(&mut self, command: Command) {
        self.e.handle(self.now, Event::Command(command));
        self.collect();
    }
    pub fn place(&mut self, raw: &str) {
        let orders = serde_json::from_str(raw).unwrap();
        self.command(Command::PlaceOrders { command: 7, orders });
    }
    pub fn failed(&self) -> bool {
        self.notices
            .iter()
            .any(|n| matches!(n, Notice::CommandFailed { command: 7, .. }))
    }
    pub fn result(&self) -> Option<&str> {
        self.notices.iter().find_map(|n| match n {
            Notice::CommandDone {
                command: 7,
                result: v,
            } => Some(v.as_str()),
            _ => None,
        })
    }
    pub fn writes(&self) -> usize {
        self.requests
            .iter()
            .filter(|r| {
                r.method == Method::Post
                    && (r.url.ends_with("/fapi/v2/order") || r.url.ends_with("/fapi/v2/cancel"))
            })
            .count()
    }
    pub fn order(&mut self, filled: f64) {
        self.e.store_mut().upsert_order(Order {
            id: "2334151155877155281".into(),
            parent_id: None,
            status: OrderStatus::Open,
            symbol: "E-BTC-USDT".into(),
            kind: OrderType::Limit,
            side: OrderSide::Buy,
            price: 2000.0,
            amount: 0.01,
            filled,
            remaining: 0.01 - filled,
            reduce_only: false,
            close: None,
            client_order_id: Some("callerOld".into()),
            time_in_force: Some(TimeInForce::GoodTillCancel),
            last_update: 0.0,
            trigger_type: None,
            take_profit: None,
            stop_loss: None,
            extra: Some(OrderExtra {
                remaining_native: Some(0.01 - filled),
                filled_in_placement_units: Some(filled),
                ..OrderExtra::default()
            }),
        });
    }
    pub fn amend(&mut self) {
        self.command(Command::AmendOrder {
            command: 7,
            id: "2334151155877155281".into(),
            price: Some(1990.0),
            amount: None,
        });
    }
}
pub fn body(request: &HttpRequest) -> serde_json::Value {
    serde_json::from_str(request.body.as_ref().unwrap()).unwrap()
}
pub fn detail(status: &str, filled: f64, extra: &str) -> String {
    format!(
        r#"{{"code":"0","data":[{{"orderId":2334151155877155281,"origQty":0.01,"executedQty":{filled},"type":"LIMIT","status":"{status}","side":"BUY","action":"OPEN"{extra}}}]}}"#
    )
}
