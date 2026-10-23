//! listenKey create and keepalive run as engine session writes. No
//! credentials or live requests are used.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Response, SocketId, TimerId};
mod support;
use safe_cex_venue_asterdex::AsterDex;
use support::Options;

const KEY: &str = "/listenKey";
const MARKETS: &str = r#"{"symbols":[{"symbol":"BTCUSDT","status":"TRADING","baseAsset":"BTC","quoteAsset":"USDT","pricePrecision":2,"quantityPrecision":3,"filters":[{"filterType":"LOT_SIZE","minQty":"0.001","maxQty":"1000"}]}]}"#;
const BALANCE: &str = r#"{"totalWalletBalance":"1000","availableBalance":"800","totalInitialMargin":"100","totalOpenOrderInitialMargin":"20","totalUnrealizedProfit":"5"}"#;

struct H {
    engine: Engine<AsterDex>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<SocketId>,
    timers: Vec<(TimerId, f64)>,
    closed: Vec<SocketId>,
}

impl H {
    /// Boots to an open private stream, leaving the create in `requests`
    /// until `create` answers it.
    fn started() -> Self {
        let mut h = Self {
            engine: support::engine(Options {
                key: Some("documented-key".into()),
                secret: Some("synthetic-secret".into()),
                ..Default::default()
            }),
            now: 1700000000000.0,
            requests: vec![],
            sockets: vec![],
            timers: vec![],
            closed: vec![],
        };
        h.engine.start(h.now);
        h.collect();
        h.reply("/time", r#"{"serverTime":1700000000000}"#);
        h.reply("/exchangeInfo", MARKETS);
        h.reply("/positionSide/dual", r#"{"dualSidePosition":false}"#);
        h.reply("/leverageBracket", "[]");
        h.reply("/ticker/24hr", "[]");
        h.reply("/openInterest", "[]");
        h.reply("/account", BALANCE);
        h.reply("/positionRisk", "[]");
        h.reply("/openOrders", "[]");
        h
    }
    fn open_private(&mut self) -> SocketId {
        self.reply(KEY, r#"{"listenKey":"private-contract"}"#);
        let socket = *self.sockets.last().expect("private socket");
        self.event(Event::WsOpened { socket });
        self.requests.retain(|r| !r.url.contains("/openOrders"));
        socket
    }
    fn collect(&mut self) {
        for action in support::drain(&mut self.engine, self.now) {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::WsConnect { socket, url, .. } if url.contains("/ws/") => {
                    self.sockets.push(socket)
                }
                Action::WsClose { socket } => self.closed.push(socket),
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
    fn take(&mut self, suffix: &str) -> HttpRequest {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.split('?').next().unwrap().ends_with(suffix))
            .unwrap_or_else(|| panic!("missing {suffix}: {:?}", self.requests));
        self.requests.remove(i)
    }
    fn reply(&mut self, suffix: &str, body: &str) {
        let r = self.take(suffix);
        self.event(Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        });
    }
    fn keys(&self) -> Vec<&HttpRequest> {
        self.requests
            .iter()
            .filter(|r| r.url.split('?').next().unwrap().ends_with(KEY))
            .collect()
    }
    /// Fire every due timer in time order, as a host would, until `done`.
    fn run_until(&mut self, end: f64, done: impl Fn(&Self) -> bool) {
        while !done(self) {
            let Some(&(timer, at)) = self
                .timers
                .iter()
                .filter(|(_, at)| *at <= end)
                .min_by(|a, b| a.1.total_cmp(&b.1))
            else {
                return;
            };
            self.timers.retain(|(t, _)| *t != timer);
            self.now = self.now.max(at);
            self.event(Event::TimerFired { timer });
            // A live stream answers the supervisor's subscription ping.
            if let Some(&socket) = self.sockets.last()
                && !self.closed.contains(&socket)
            {
                self.event(Event::WsMessage {
                    socket,
                    text: r#"{"id":42,"result":[]}"#,
                });
            }
            if self.requests.iter().any(|r| r.url.ends_with("/time")) {
                let time = format!(r#"{{"serverTime":{}}}"#, self.now);
                self.reply("/time", &time);
            }
        }
    }
}

#[test]
#[ignore = "original RED: listenKey upkeep is unmanaged legacy HTTP until it adopts the session core"]
fn listen_key_create_carries_its_session_bound() {
    let h = H::started();
    let [create] = h.keys()[..] else {
        panic!("one create: {:?}", h.requests);
    };
    assert_eq!(create.method, Method::Post);
    assert_eq!(
        create.timeout_ms,
        Some(20_000),
        "the engine-owned raw bound"
    );
}

#[test]
#[ignore = "original RED: a hung legacy keepalive has no engine bound"]
fn a_hung_keepalive_is_retried_at_its_bound() {
    let mut h = H::started();
    let socket = h.open_private();
    let start = h.now;
    h.run_until(start + 1_800_000., |h| !h.keys().is_empty());
    let [put] = h.keys()[..] else {
        panic!("one keepalive: {:?}", h.requests);
    };
    assert_eq!(put.method, Method::Put);
    let sent = h.now;
    // The keepalive is never answered and the host reports nothing.
    h.run_until(sent + 21_500., |h| h.keys().len() > 1);
    assert_eq!(
        h.keys().len(),
        2,
        "the engine bounds the keepalive and the venue retries it"
    );
    assert!(h.keys().iter().all(|r| r.method == Method::Put));
    assert!(!h.closed.contains(&socket), "the private stream stays up");
}
