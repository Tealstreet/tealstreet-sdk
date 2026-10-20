//! Private reported PnL remains authoritative before and after public quotes.
mod support;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, TimerId};
use safe_cex_venue_apexomni::{ApexOmni, Options};
use support::{MARKETS, TestEngine, response};
const BALANCE: &str = r#"{"code":0,"data":{"totalEquityValue":"1000","initialMargin":"100","maintenanceMargin":"20"}}"#;
fn account(pnl: Option<f64>) -> String {
    let pnl = pnl.map_or_else(String::new, |p| format!(",\"unrealisedPnl\":\"{p}\""));
    format!(
        r#"{{"code":0,"data":{{"id":"611779989544632629","positions":[{{"symbol":"BTC-USDT","side":"LONG","entryPrice":"100","size":"2","customImr":"0.1","curRealisedPnl":"999999"{pnl}}}],"contractAccount":{{"makerFeeRate":"0.0002","takerFeeRate":"0.0005"}}}}}}"#
    )
}
struct Harness {
    engine: TestEngine,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    logs: Vec<String>,
    now: f64,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self {
            engine: Engine::new(ApexOmni::new(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                passphrase: Some("pass".into()),
                omni_key_seed: Some("11".repeat(65)),
                ..Options::default()
            })),
            requests: vec![],
            timers: vec![],
            logs: vec![],
            now: 0.0,
        };
        h.engine.start(0.0);
        h.collect();
        h.reply("/v3/symbols", MARKETS);
        h
    }
    fn collect(&mut self) {
        for action in self.engine.drain_actions() {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::Notice(Notice::Log { message, .. }) => self.logs.push(message),
                _ => {}
            }
        }
    }
    fn reply(&mut self, path: &str, body: &str) {
        let i = self
            .requests
            .iter()
            .position(|r| r.url.ends_with(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.requests));
        let request = self.requests.remove(i);
        response(&mut self.engine, &request, self.now, body);
        self.collect();
    }
    fn snapshot(&mut self, pnl: Option<f64>) {
        self.reply("/v3/account", &account(pnl));
        self.reply("/v3/open-orders", r#"{"code":0,"data":[]}"#);
        self.reply("/v3/account-balance", BALANCE);
    }
    fn poll(&mut self) {
        let i = self
            .timers
            .iter()
            .position(|(_, at)| *at == self.now + 2000.0)
            .expect("account poll");
        let (timer, at) = self.timers.remove(i);
        self.now = at;
        self.engine.handle(at, Event::TimerFired { timer });
        self.collect();
    }
}
#[test]
fn native_position_pnl_drives_funded_balance_even_with_zero_invalid_or_conflicting_quotes() {
    for price in [0.0, -1.0, f64::NAN, f64::INFINITY, 900.0] {
        let mut h = Harness::new();
        h.engine
            .store_mut()
            .update_ticker("BTCUSDT", |t| t.last = price);
        h.snapshot(Some(4.0));
        let p = &h.engine.store().positions()[0];
        assert_eq!((p.contracts, p.unrealized_pnl), (2.0, 4.0));
        assert_eq!(p.realized_pnl, Some(999999.0));
        let b = h.engine.store().balance();
        assert_eq!((b.total, b.free, b.used, b.upnl), (996.0, 900.0, 20.0, 4.0));
        h.poll();
        h.snapshot(Some(8.0));
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 8.0);
        assert_eq!(h.engine.store().balance().upnl, 8.0);
        h.poll();
        h.snapshot(None);
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 8.0);
        assert_eq!(h.engine.store().balance().upnl, 8.0);
        assert!(h.logs.iter().any(|s| s.contains("PnL unavailable")));
    }
}
#[test]
fn missing_initial_native_pnl_keeps_exposure_without_using_entry_as_a_market_price() {
    let mut h = Harness::new();
    h.snapshot(None);
    assert_eq!(h.engine.store().positions()[0].contracts, 2.0);
    assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 0.0);
    assert_eq!(h.engine.store().balance().upnl, 0.0);
    assert!(h.logs.iter().any(|s| s.contains("PnL unavailable")));
}
