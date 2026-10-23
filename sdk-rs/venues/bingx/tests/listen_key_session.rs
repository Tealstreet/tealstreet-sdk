//! Listen-key upkeep runs as engine session writes. No credentials or live
//! requests are used.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Method, Notice, Response, TimerId};
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
    /// Fire the earliest timer due at or after `from`, advancing the clock.
    fn fire_next(&mut self, from: f64) -> f64 {
        let i = (0..self.timers.len())
            .filter(|&i| self.timers[i].1 >= from)
            .min_by(|&a, &b| self.timers[a].1.total_cmp(&self.timers[b].1))
            .unwrap_or_else(|| panic!("no timer at or after {from}; have {:?}", self.timers));
        let (timer, at) = self.timers.remove(i);
        self.now = at;
        self.engine.handle(at, Event::TimerFired { timer });
        self.collect();
        at
    }
}

#[test]
fn a_hung_listen_key_create_fails_at_its_bound_and_retries() {
    let mut h = Harness::started();
    let create = h.keys()[0].clone();
    assert_eq!(create.method, Method::Post);
    let sent = h.now;
    let fired = h.fire_next(sent + 20_000.0);
    assert_eq!(fired, sent + 20_000.0, "the create is bounded at 20 s");
    assert!(
        h.logs
            .iter()
            .any(|m| m.starts_with("BingX private request failed")),
        "{:?}",
        h.logs
    );
    h.requests.retain(|r| r.id != create.id);
    while h.keys().is_empty() {
        let at = h.fire_next(h.now);
        assert!(
            at <= fired + 60_000.0,
            "no fresh create within the native backoff"
        );
    }
    let retried = h.keys();
    assert_eq!(
        retried.len(),
        1,
        "one fresh create after the native backoff"
    );
    assert_eq!(retried[0].method, Method::Post);
}

#[test]
fn listen_key_requests_present_only_the_api_key() {
    let h = Harness::started();
    let create = h.keys()[0];
    assert!(
        create
            .headers
            .iter()
            .any(|(name, _)| *name == "X-BX-APIKEY")
            || create
                .signer_headers
                .iter()
                .any(|(name, value)| name == "X-BX-APIKEY" && value == "key"),
        "{create:?}"
    );
    assert!(
        !create.url.contains("signature="),
        "unsigned listen key: {}",
        create.url
    );
}
