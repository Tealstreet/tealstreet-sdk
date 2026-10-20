//! Funded private snapshots must remain usable before public prices arrive.
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Notice, Response, TimerId};
use safe_cex_venue_bitrue::{Bitrue, Options};

const MARKETS: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
fn account(side: &str, size: f64) -> String {
    format!(
        r#"{{"code":"0","data":{{"account":[{{"marginCoin":"USDT","totalEquity":1000,"accountNormal":800,"accountLock":5,"sumOpenRealizedAmount":999999,"positionVos":[{{"contractName":"E-BTC-USDT","positions":[{{"side":"{side}","volume":{size},"openPrice":100,"leverageLevel":25,"positionType":2,"positionBalance":200,"forceLiquidationPrice":50}}]}}]}}]}}}}"#
    )
}
struct Harness {
    engine: Engine<Bitrue>,
    requests: Vec<HttpRequest>,
    timers: Vec<(TimerId, f64)>,
    logs: Vec<String>,
    now: f64,
}
impl Harness {
    fn new() -> Self {
        let mut h = Self {
            engine: Engine::new(Bitrue::new(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            })),
            requests: vec![],
            timers: vec![],
            logs: vec![],
            now: 0.0,
        };
        h.engine.start(0.0);
        h.collect();
        h.reply("/contracts", MARKETS);
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
            .position(|r| r.url.contains(path))
            .unwrap_or_else(|| panic!("missing {path}: {:?}", self.requests));
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
fn funded_snapshot_retains_native_exposure_without_fabricating_an_entry_loss() {
    for price in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        let mut h = Harness::new();
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = price);
        h.reply("/account", &account("BUY", 2.0));
        let p = &h.engine.store().positions()[0];
        assert_eq!(
            (p.contracts, p.entry_price, p.unrealized_pnl),
            (2.0, 100.0, 0.0)
        );
        let b = h.engine.store().balance();
        assert_eq!(
            (b.total, b.free, b.used, b.upnl),
            (1000.0, 800.0, 200.0, 0.0)
        );
        assert!(
            h.logs
                .iter()
                .any(|s| s.contains("ticker price unavailable"))
        );
        assert!(h.logs.iter().all(|s| !s.contains("999999")));
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
        h.poll();
        h.reply("/account", &account("BUY", 2.0));
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 20.0);
        assert_eq!(h.engine.store().balance().upnl, 20.0);
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = price);
        h.poll();
        h.reply("/account", &account("BUY", 3.0));
        assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 20.0);
        assert_eq!(h.engine.store().balance().upnl, 20.0);
    }
}
#[test]
fn an_unmappable_position_cannot_publish_a_partial_account_upnl_sum() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    h.reply("/account", &account("BUY", 2.0));
    let positions = h.engine.store().positions().to_vec();
    let balance = h.engine.store().balance().clone();
    h.poll();
    h.reply(
        "/account",
        &account("BUY", 2.0).replace("E-BTC-USDT", "UNKNOWN"),
    );
    assert_eq!(h.engine.store().positions(), positions);
    assert_eq!(h.engine.store().balance(), &balance);
}
#[test]
fn a_new_opposite_side_does_not_inherit_the_previous_positions_valuation() {
    let mut h = Harness::new();
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    h.reply("/account", &account("BUY", 2.0));
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 0.0);
    h.poll();
    h.reply("/account", &account("SELL", 3.0));
    assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 0.0);
    assert_eq!(h.engine.store().balance().upnl, 0.0);
}
#[test]
fn empty_groups_for_uncatalogued_contracts_do_not_block_the_snapshot() {
    for empty in [
        r#"{"contractName":"E-GONE-USDT","positions":[]}"#,
        r#"{"contractName":"E-GONE-USDT","positions":[{"side":"BUY","volume":0,"openPrice":0,"leverageLevel":5,"positionType":1}]}"#,
    ] {
        let mut h = Harness::new();
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
        let body = account("BUY", 2.0)
            .replace(r#""positionVos":["#, &format!(r#""positionVos":[{empty},"#));
        h.reply("/account", &body);
        assert_eq!(h.engine.store().positions().len(), 1);
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 20.0);
        assert_eq!(h.engine.store().balance().total, 1000.0);
    }
}
