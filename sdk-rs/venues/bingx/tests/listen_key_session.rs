//! Listen-key upkeep runs as engine session writes. No credentials or live
//! requests are used.
use safe_cex_engine::{
    Action, Engine, Event, HttpRequest, Method, Notice, Response, SocketId, TimerId,
};
use safe_cex_venue_bingx::Bingx;
mod support;
use support::Options;

const KEY: &str = "/openApi/user/auth/userDataStream";
const MARKETS: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","asset":"BTC","currency":"USDT","status":1,"quantityPrecision":4,"pricePrecision":1,"tradeMinLimit":"0.0001","maxLongLeverage":125}]}"#;
const DAILY: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","lastPrice":"60000","priceChangePercent":"1","volume":"12","quoteVolume":"720000"}]}"#;
const PREMIUM: &str = r#"{"code":0,"data":[{"symbol":"BTC-USDT","markPrice":"60001","indexPrice":"60002","lastFundingRate":"0.01","nextFundingTime":1700000000000}]}"#;

struct Harness {
    engine: Engine<Bingx>,
    now: f64,
    requests: Vec<HttpRequest>,
    sockets: Vec<(SocketId, String)>,
    timers: Vec<(TimerId, f64)>,
    logs: Vec<String>,
}

impl Harness {
    fn started() -> Self {
        let mut h = Self {
            engine: support::engine(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            }),
            now: 1000.0,
            requests: Vec::new(),
            sockets: Vec::new(),
            timers: Vec::new(),
            logs: Vec::new(),
        };
        h.engine.start(h.now);
        h.collect();
        h.respond("/quote/contracts", MARKETS);
        h.respond("/quote/ticker", DAILY);
        h.respond("/quote/premiumIndex", PREMIUM);
        h
    }
    fn collect(&mut self) {
        for action in safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now) {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::WsConnect { socket, url, .. } => self.sockets.push((socket, url)),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::ClearTimer { timer } => self.timers.retain(|(t, _)| *t != timer),
                Action::Notice(Notice::Event(event)) => self.logs.push(event.message),
                _ => {}
            }
        }
    }
    fn respond(&mut self, path: &str, body: &str) {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.contains(path))
            .unwrap();
        let request = self.requests.remove(i);
        self.engine.handle(
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
    fn keys(&self) -> Vec<&HttpRequest> {
        self.requests
            .iter()
            .filter(|r| r.url.contains(KEY))
            .collect()
    }
    /// The private socket from the last accepted listen key, opened.
    fn open_private(&mut self) -> SocketId {
        let socket = self
            .sockets
            .iter()
            .rev()
            .find(|(_, url)| url.contains("listenKey="))
            .expect("private socket")
            .0;
        self.engine.handle(self.now, Event::WsOpened { socket });
        self.collect();
        socket
    }
    fn private_traffic_until(&mut self, until: f64, socket: SocketId) {
        while self.now < until {
            self.now = (self.now + 30_000.).min(until);
            self.engine.handle(
                self.now,
                Event::WsMessage {
                    socket,
                    text: "Pong",
                },
            );
            self.collect();
        }
    }
    /// Fire every timer due by `until` in chronological order, with private
    /// traffic before each so an idle watchdog, if armed, stays satisfied.
    fn run_until(&mut self, until: f64, socket: SocketId, stop: impl Fn(&Self) -> bool) {
        while !stop(self) {
            let Some(i) = (0..self.timers.len())
                .filter(|&i| self.timers[i].1 <= until)
                .min_by(|&a, &b| self.timers[a].1.total_cmp(&self.timers[b].1))
            else {
                return;
            };
            let (timer, at) = self.timers.remove(i);
            self.now = self.now.max(at);
            self.engine.handle(
                self.now,
                Event::WsMessage {
                    socket,
                    text: "Pong",
                },
            );
            self.engine.handle(self.now, Event::TimerFired { timer });
            self.collect();
        }
    }
}

#[test]
fn a_hung_listen_key_keepalive_fails_at_its_bound_and_retries() {
    let mut h = Harness::started();
    h.respond(KEY, r#"{"listenKey":"key-one"}"#);
    let socket = h.open_private();
    let (timer, at) = *h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 30.0 * 60_000.0)
        .expect("30 min keepalive");
    // Skip the idle 30 minutes: drop every other pending timer (polls,
    // supervisor watch/ping) on purpose, so only the keepalive's own bound and
    // retry run. A full-order probe gives the same outcome.
    h.timers.retain(|(t, due)| *t != timer && *due >= at);
    h.requests.clear();
    h.private_traffic_until(at, socket);
    h.engine.handle(
        at,
        Event::WsMessage {
            socket,
            text: "Pong",
        },
    );
    h.engine.handle(at, Event::TimerFired { timer });
    h.collect();
    let put = h.keys()[0].clone();
    assert_eq!(put.method, Method::Put);
    let sent = h.now;
    let before = h.logs.len();
    let failed = move |h: &Harness| {
        h.logs[before..]
            .iter()
            .any(|m| m.starts_with("BingX private request failed"))
    };
    h.run_until(sent + 20_000.0, socket, failed);
    assert!(
        failed(&h),
        "a hung keepalive fails at its 20 s bound: {:?}",
        h.logs
    );
    assert_eq!(h.now, sent + 20_000.0);
    h.requests.retain(|r| r.id != put.id);
    h.run_until(sent + 20_000.0 + 30_000.0, socket, |h| !h.keys().is_empty());
    let retried = h.keys();
    assert_eq!(
        retried.len(),
        1,
        "one keepalive after the native 30 s retry"
    );
    assert_eq!(retried[0].method, Method::Put);
    assert!(
        retried[0].url.ends_with("?listenKey=key-one"),
        "{}",
        retried[0].url
    );
}

#[test]
fn listen_key_requests_present_only_the_api_key() {
    let mut h = Harness::started();
    let create = h.keys()[0].clone();
    assert_eq!(create.method, Method::Post);
    assert!(create.url.ends_with(KEY), "{}", create.url);
    assert_eq!(create.body, None);
    let headers = |r: &HttpRequest| {
        let mut all = r
            .headers
            .iter()
            .map(|(name, value)| ((*name).to_owned(), value.clone()))
            .chain(r.signer_headers.iter().cloned())
            .collect::<Vec<_>>();
        all.sort();
        all
    };
    let presented = headers(&create);
    assert!(
        presented.contains(&("X-BX-APIKEY".into(), "key".into())),
        "{presented:?}"
    );
    assert!(
        presented.iter().all(|(name, _)| !name.contains("SIGN")),
        "{presented:?}"
    );
    assert!(!create.url.contains("signature=") && !create.url.contains("timestamp="));
    h.respond(KEY, r#"{"listenKey":"key-one"}"#);
    let socket = h.open_private();
    let (timer, at) = *h
        .timers
        .iter()
        .find(|(_, at)| *at == h.now + 30.0 * 60_000.0)
        .expect("30 min keepalive");
    h.private_traffic_until(at, socket);
    h.engine.handle(
        at,
        Event::WsMessage {
            socket,
            text: "Pong",
        },
    );
    h.engine.handle(at, Event::TimerFired { timer });
    h.collect();
    let put = h.keys()[0].clone();
    assert_eq!(put.method, Method::Put);
    assert!(
        put.url.ends_with(&format!("{KEY}?listenKey=key-one")),
        "{}",
        put.url
    );
    assert_eq!(put.body, None);
    let presented = headers(&put);
    assert!(
        presented.contains(&("X-BX-APIKEY".into(), "key".into())),
        "{presented:?}"
    );
    assert!(
        presented.iter().all(|(name, _)| !name.contains("SIGN")),
        "{presented:?}"
    );
    assert!(!put.url.contains("signature=") && !put.url.contains("timestamp="));
}
