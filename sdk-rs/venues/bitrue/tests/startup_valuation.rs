//! Funded private snapshots must remain usable before public prices arrive.
mod support;
use safe_cex_engine::{Action, Engine, Event, HttpRequest, Response, TimerId};
use safe_cex_venue_bitrue::Bitrue;
use support::Options;

const MARKETS: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
// E-BAD-USDT is skipped as malformed (zero multiplier).
const PARTIAL_MARKETS: &str = r#"[{"symbol":"E-BTC-USDT","pricePrecision":1,"multiplier":1,"minOrderVolume":1,"maxMarketVolume":10000,"status":1},{"symbol":"E-BAD-USDT","pricePrecision":2,"multiplier":0,"minOrderVolume":1,"maxMarketVolume":10000,"status":1}]"#;
fn account(side: &str, size: f64) -> String {
    format!(
        r#"{{"code":"0","data":{{"account":[{{"marginCoin":"USDT","totalEquity":1000,"accountNormal":800,"accountLock":5,"sumOpenRealizedAmount":-0.5,"positionVos":[{{"contractName":"E-BTC-USDT","positions":[{{"side":"{side}","volume":{size},"openPrice":100,"leverageLevel":25,"positionType":2,"positionBalance":200,"forceLiquidationPrice":50}}]}}]}}]}}}}"#
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
        Self::with_markets(MARKETS)
    }
    fn with_markets(markets: &str) -> Self {
        let mut h = Self {
            engine: support::engine(Options {
                key: Some("key".into()),
                secret: Some("secret".into()),
                ..Options::default()
            }),
            requests: vec![],
            timers: vec![],
            logs: vec![],
            now: 0.0,
        };
        h.engine.start(0.0);
        h.collect();
        h.reply("/contracts", markets);
        h
    }
    fn collect(&mut self) {
        for action in
            safe_cex_engine::test_support::prepared_actions(&mut self.engine, self.now).into_iter()
        {
            match action {
                Action::Http(r) => self.requests.push(r),
                Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    message,
                    ..
                })) => self.logs.push(message),
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
            (1000.0, 800.0, 200.0, -0.5)
        );
        assert!(
            h.logs
                .iter()
                .any(|s| s.contains("ticker price unavailable"))
        );
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
        h.poll();
        h.reply("/account", &account("BUY", 2.0));
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 20.0);
        assert_eq!(h.engine.store().balance().upnl, -0.5);
        h.engine
            .store_mut()
            .update_ticker("E-BTC-USDT", |t| t.last = price);
        h.poll();
        h.reply("/account", &account("BUY", 3.0));
        assert_eq!(h.engine.store().positions()[0].contracts, 3.0);
        assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 20.0);
        assert_eq!(h.engine.store().balance().upnl, -0.5);
    }
}
#[test]
fn an_unmappable_position_retains_the_prior_account_snapshot() {
    let mut h = Harness::with_markets(PARTIAL_MARKETS);
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    h.reply("/account", &account("BUY", 2.0));
    let positions = h.engine.store().positions().to_vec();
    let balance = h.engine.store().balance().clone();
    h.poll();
    h.reply(
        "/account",
        &account("BUY", 2.0).replace("E-BTC-USDT", "E-BAD-USDT"),
    );
    assert_eq!(h.engine.store().positions(), positions);
    assert_eq!(h.engine.store().balance(), &balance);
}
#[test]
fn owned_position_on_uncatalogued_contract_is_skipped_and_the_snapshot_commits() {
    let mut h = Harness::with_markets(PARTIAL_MARKETS);
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    let gone = r#"{"contractName":"E-GONE-USDT","positions":[{"side":"BUY","volume":4,"openPrice":10,"leverageLevel":5,"positionType":1}]}"#;
    let body =
        account("BUY", 2.0).replace(r#""positionVos":["#, &format!(r#""positionVos":[{gone},"#));
    h.reply("/account", &body);
    let store = h.engine.store();
    assert!(store.loaded().positions && store.loaded().balance);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "E-BTC-USDT");
    assert_eq!(store.balance().total, 1000.0);
    assert!(
        h.logs
            .iter()
            .any(|s| s.contains("Skipping Bitrue position on uncatalogued contract E-GONE-USDT"))
    );
    h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
    assert!(h.engine.store().loaded().orders);
}
#[test]
fn partial_catalogue_commits_then_rereads_without_a_second_private_start() {
    let mut h = Harness::with_markets(PARTIAL_MARKETS);
    assert!(h.engine.store().market("E-BAD-USDT").is_none());
    let (retry, due) = take_retry(&mut h, 1000.0);
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    h.reply("/account", &account("BUY", 2.0));
    h.reply("/openOrders", r#"{"code":"0","data":[]}"#);
    h.requests.clear();
    h.now = due;
    h.engine.handle(due, Event::TimerFired { timer: retry });
    h.collect();
    let before = h.timers.len();
    h.reply(
        "/contracts",
        &PARTIAL_MARKETS.replace(r#""multiplier":0"#, r#""multiplier":1"#),
    );
    assert!(
        h.engine.store().market("E-BAD-USDT").is_some(),
        "repaired market"
    );
    assert!(h.engine.store().market("E-BTC-USDT").is_some());
    assert!(
        h.requests.is_empty(),
        "no private restart: {:?}",
        h.requests
    );
    assert_eq!(h.timers.len(), before, "no further catalogue retry");
    assert!(h.engine.store().loaded().positions);
    // Tickers are not reseeded: the live price survives the refresh.
    assert_eq!(h.engine.store().ticker("E-BTC-USDT").unwrap().last, 110.0);
    assert_eq!(h.engine.store().tickers().len(), 1);
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
    assert_eq!(h.engine.store().balance().upnl, -0.5);
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
const ORDER: &str = r#"{"code":"0","data":[{"orderId":2334151155877155281,"clientOrderId":"caller","type":"LIMIT","side":"BUY","status":"NEW","action":"OPEN","price":100,"origQty":100,"executedQty":0,"transactTime":1}]}"#;
/// Retry delays are jittered into `[0.75, 1.0] * nominal`.
fn jittered(actual: f64, nominal: f64) -> bool {
    (0.75 * nominal..=nominal).contains(&actual)
}
/// Takes the startup retry jittered from `nominal` after now, with its due time.
fn take_retry(h: &mut Harness, nominal: f64) -> (TimerId, f64) {
    let i = h
        .timers
        .iter()
        .position(|(_, at)| jittered(*at - h.now, nominal))
        .unwrap_or_else(|| panic!("no retry jittered from {nominal}: {:?}", h.timers));
    h.timers.remove(i)
}
fn take_timer(h: &mut Harness, at: f64) -> TimerId {
    let i = h
        .timers
        .iter()
        .position(|(_, t)| *t == at)
        .unwrap_or_else(|| panic!("no timer at {at}: {:?}", h.timers));
    h.timers.remove(i).0
}
#[test]
fn refresh_that_omits_a_held_contract_keeps_it_with_its_position_and_order() {
    let mut h = Harness::with_markets(PARTIAL_MARKETS);
    let (retry, due) = take_retry(&mut h, 1000.0);
    h.engine
        .store_mut()
        .update_ticker("E-BTC-USDT", |t| t.last = 110.0);
    h.reply("/account", &account("BUY", 2.0));
    h.reply("/openOrders", ORDER);
    assert!(h.engine.store().order("2334151155877155281").is_some());
    let listed = h.engine.store().market("E-BTC-USDT").cloned().unwrap();
    h.now = due;
    h.engine.handle(due, Event::TimerFired { timer: retry });
    h.collect();
    // E-BTC-USDT drops out of the list while E-BAD-USDT stays malformed.
    h.reply(
        "/contracts",
        &PARTIAL_MARKETS.replace("E-BTC-USDT", "E-ETH-USDT"),
    );
    assert_eq!(h.engine.store().market("E-BTC-USDT"), Some(&listed));
    assert!(h.engine.store().market("E-ETH-USDT").is_some());
    let timer = take_timer(&mut h, 2000.0);
    h.now = 2000.0;
    h.engine.handle(2000.0, Event::TimerFired { timer });
    h.collect();
    h.reply("/account", &account("BUY", 2.0));
    let store = h.engine.store();
    assert!(store.loaded().positions && store.loaded().balance && store.loaded().orders);
    assert_eq!(store.positions().len(), 1);
    assert_eq!(store.positions()[0].symbol, "E-BTC-USDT");
    assert!(store.order("2334151155877155281").is_some());
}
#[test]
fn persistently_malformed_catalogue_stops_rereading_after_five_refreshes() {
    let mut h = Harness::with_markets(PARTIAL_MARKETS);
    let mut retry = Some(take_retry(&mut h, 1000.0));
    let mut rereads = 0;
    while let Some((timer, at)) = retry.take() {
        h.now = at;
        h.engine.handle(at, Event::TimerFired { timer });
        h.collect();
        let before = h.timers.len();
        h.reply("/contracts", PARTIAL_MARKETS);
        rereads += 1;
        retry = h.timers.get(before).copied();
        h.timers.truncate(before);
        assert!(rereads <= 5, "re-read is bounded");
    }
    assert_eq!(rereads, 5);
    assert!(h.logs.iter().any(|s| s.contains("stops re-reading")));
}
#[test]
fn private_tape_startup_keeps_realized_currency_upnl_before_the_first_quote() {
    let mut h = Harness::new();
    let usdc = r#"{"marginCoin":"USDC","totalEquity":40,"accountNormal":35,"accountLock":2,"sumOpenRealizedAmount":-0.5,"positionVos":[]}"#;
    h.reply(
        "/account",
        &account("BUY", 2.0).replace("}]}]}]}}", &format!("}}]}}]}},{usdc}]}}}}")),
    );
    // No quote yet: entry 100 against a zero placeholder is not a 200 loss.
    assert_eq!(h.engine.store().positions()[0].unrealized_pnl, 0.0);
    let coins = h.engine.store().currency_balances().unwrap();
    let upnl = |c: &str| coins.iter().find(|b| b.currency == c).unwrap().balance.upnl;
    assert_eq!((upnl("USDC"), upnl("USDT")), (-0.5, -0.5));
}
