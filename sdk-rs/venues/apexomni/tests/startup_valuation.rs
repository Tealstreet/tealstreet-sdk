//! Live last prices value the balance like TS; reported PnL covers absent or placeholder quotes.
mod support;
use safe_cex_engine::{Action, Event, HttpRequest, Notice, TimerId};
use safe_cex_venue_apexomni::Options;
use support::{MARKETS, Material, Prepared, TestEngine, private_engine, response};
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
            engine: private_engine(
                Options {
                    ..Options::default()
                },
                &Material {
                    key: "key",
                    signing: "secret",
                    passphrase: Some("pass"),
                    seed: Some(("11".repeat(65)).to_string()),
                },
            ),
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
        for action in self.engine.prepared(self.now) {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::Notice(Notice::Event(safe_cex_engine::LogEvent { message, .. })) => {
                    self.logs.push(message)
                }
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
fn native_position_pnl_drives_funded_balance_with_placeholder_or_invalid_quotes() {
    for price in [0.0, -1.0, f64::NAN, f64::INFINITY] {
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
fn real_last_price_values_the_balance_even_when_native_position_pnl_lags() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .update_ticker("BTCUSDT", |t| t.last = 95.0);
    h.snapshot(Some(0.0));
    assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 0.0);
    let b = h.engine.store().balance();
    assert_eq!(
        (b.total, b.free, b.used, b.upnl),
        (1010.0, 900.0, 20.0, -10.0)
    );
    h.poll();
    h.snapshot(None);
    assert_eq!(h.engine.store().balance().upnl, -10.0);
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

#[test]
fn unmappable_owned_position_cannot_replace_a_funded_snapshot_with_empty() {
    let mut h = Harness::new();
    h.snapshot(Some(4.0));
    let before = h.engine.store().positions().to_vec();
    h.poll();
    h.reply(
        "/v3/account",
        &account(Some(4.0)).replace("BTC-USDT", "UNKNOWN"),
    );
    assert_eq!(h.engine.store().positions(), before);
    assert!(
        h.logs
            .iter()
            .any(|s| s.contains("skipped malformed position row UNKNOWN;"))
    );
}

#[test]
fn unmappable_owned_order_cannot_clear_the_existing_order_snapshot() {
    let mut h = Harness::new();
    h.reply("/v3/account", &account(Some(4.0)));
    let known = r#"{"code":0,"data":[{"id":"owned","symbol":"BTC-USDT","status":"OPEN","type":"LIMIT","side":"BUY","size":"2","price":"100","cumSuccessFillSize":"0","timeInForce":"GOOD_TIL_CANCEL"}]}"#;
    h.reply("/v3/open-orders", known);
    h.reply("/v3/account-balance", BALANCE);
    let before = h.engine.store().orders().to_vec();
    assert_eq!(before.len(), 1);
    h.poll();
    h.reply("/v3/account", &account(Some(4.0)));
    h.reply("/v3/open-orders", &known.replace("BTC-USDT", "UNKNOWN"));
    assert_eq!(h.engine.store().orders(), before);
    assert!(
        h.logs
            .iter()
            .any(|s| s.contains("skipped malformed order row owned;"))
    );
}
