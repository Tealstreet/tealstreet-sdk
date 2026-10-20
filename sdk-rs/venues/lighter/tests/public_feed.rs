//! Trusted public quotes retain native private position valuation.
mod support;
use safe_cex_engine::{Action, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId};
use safe_cex_venue_lighter::Lighter;
use support::*;

const STATS: &str = r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","mark_price":"100","index_price":"100"},{"market_id":2,"last_trade_price":"200","mark_price":"200","index_price":"200"}]}"#;
struct Rig {
    engine: Engine<Lighter>,
    public: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    reads: Vec<String>,
}
impl Rig {
    fn new(policy: PublicFeedPolicy) -> Self {
        let mut engine = Engine::new(Lighter::new(options()));
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(NOW);
        let mut rig = Self {
            engine,
            public: None,
            timers: Vec::new(),
            reads: Vec::new(),
        };
        rig.drain(NOW, STATS);
        // Without a public socket Shared still owns native metadata/REST seeds.
        // Process its real first poll, then reconcile the private snapshot.
        rig.fire_due(NOW + 2000., STATS);
        assert!(rig.engine.store().loaded().balance);
        assert!(rig.engine.store().loaded().positions);
        assert_eq!(rig.engine.store().positions()[0].contracts, 2.);
        rig
    }
    fn drain(&mut self, now: f64, stats: &str) {
        for _ in 0..50 {
            let mut actions: Vec<_> = self.engine.drain_actions().collect();
            if actions.is_empty() {
                return;
            }
            // A late native ticker response must restore provider valuations even
            // after the private snapshot in the same poll cycle has committed.
            actions.sort_by_key(
                |action| matches!(action, Action::Http(r) if r.url.contains("exchangeStats")),
            );
            for action in actions {
                match action {
                    Action::Http(request) => {
                        self.reads.push(request.url.clone());
                        let body = if request.url.contains("orderBookDetails")
                            || request.url.contains("orderBooks")
                        {
                            META
                        } else if request.url.contains("exchangeStats") {
                            stats
                        } else if request.url.contains("accountActiveOrders") {
                            OPEN
                        } else if request.url.contains("/account?") {
                            ACCOUNT
                        } else {
                            panic!("unexpected native request {}", request.url)
                        };
                        reply_at(&mut self.engine, now, &request, 200, body);
                    }
                    Action::WsConnect {
                        socket, channel, ..
                    } => {
                        if channel == Some(SocketChannel::Public) {
                            self.public = Some(socket);
                        }
                        self.engine.handle(now, Event::WsOpened { socket });
                    }
                    Action::SetTimer { timer, at_ms } => self.timers.push((timer, at_ms)),
                    Action::ClearTimer { timer } => self.timers.retain(|(id, _)| *id != timer),
                    _ => {}
                }
            }
        }
        panic!("native bootstrap did not settle");
    }
    fn fire_due(&mut self, now: f64, stats: &str) {
        let due: Vec<_> = self
            .timers
            .iter()
            .filter(|(_, at)| *at <= now)
            .copied()
            .collect();
        self.timers.retain(|(_, at)| *at > now);
        for (timer, _) in due {
            self.engine.handle(now, Event::TimerFired { timer });
        }
        self.drain(now, stats);
    }
    fn shared_quote(&mut self, last: f64, mark: f64, index: f64) {
        self.engine.apply_public_feed(NOW + 2100., serde_json::from_value(serde_json::json!({"tickers":[{"symbol":"ETH", "last":last,"mark":mark,"index":index}]})).unwrap()).unwrap();
    }
}

#[test]
fn shared_current_reference_matches_direct_position_pnl_without_replacing_private_balance() {
    let mut outcomes = Vec::new();
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        let balance = rig.engine.store().balance().clone();
        if policy == PublicFeedPolicy::Direct {
            message(
                &mut rig.engine,
                rig.public.unwrap(),
                NOW + 2100.,
                r#"{"channel":"ticker:1","ticker":{"s":"ETH","last_trade_price":"120","mark_price":"120","index_price":"120"}}"#,
            );
        } else {
            rig.shared_quote(120., 120., 120.);
        }
        let pnl = rig.engine.store().positions()[0].unrealized_pnl;
        assert_eq!(pnl, 40.);
        // The direct ticker callback updates positions only; balance retains the
        // private snapshot until reconciliation. Shared preserves that contract.
        assert_eq!(rig.engine.store().balance(), &balance);
        outcomes.push(pnl);
    }
    assert_eq!(outcomes[0], outcomes[1]);
}

#[test]
fn shared_prices_and_position_pnl_survive_native_rest_seeds_and_private_reconciliation() {
    let mut rig = Rig::new(PublicFeedPolicy::Shared);
    rig.shared_quote(120., 120., 120.);
    let reads = rig.reads.len();
    rig.fire_due(NOW + 6000., &STATS.replace("100", "55"));
    assert!(
        rig.reads[reads..]
            .iter()
            .any(|url| url.contains("exchangeStats"))
    );
    assert!(
        rig.reads[reads..]
            .iter()
            .any(|url| url.contains("/account?"))
    );
    let ticker = rig.engine.store().ticker("ETH").unwrap();
    assert_eq!((ticker.last, ticker.mark, ticker.index), (120., 120., 120.));
    assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 40.);
    assert_eq!(rig.engine.store().balance().upnl, 40.);
    assert_eq!(rig.engine.store().balance().total, 960.);
    // Cumulative shared snapshots use normalized current mark, then last.
    // Index is already folded into mark by the direct ticker mapper when absent.
    rig.shared_quote(125., 130., 128.);
    assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 60.);
    rig.shared_quote(125., 0., 128.);
    assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 50.);
    rig.shared_quote(125., 0., 0.);
    assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 50.);
}

#[test]
fn explicit_zero_mark_uses_last_in_both_direct_and_shared_modes() {
    for policy in [PublicFeedPolicy::Direct, PublicFeedPolicy::Shared] {
        let mut rig = Rig::new(policy);
        if policy == PublicFeedPolicy::Direct {
            message(
                &mut rig.engine,
                rig.public.unwrap(),
                NOW + 2100.,
                r#"{"channel":"ticker:1","ticker":{"s":"ETH","last_trade_price":"105","mark_price":"0","index_price":"109"}}"#,
            );
        } else {
            rig.shared_quote(105., 0., 109.);
        }
        assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 10.);
    }
}

#[test]
fn shared_feed_does_not_mark_private_state_loaded_before_native_snapshot() {
    let mut engine = Engine::new(Lighter::new(options()));
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(NOW);
    let request = requests(&mut engine).pop().unwrap();
    reply(&mut engine, &request, 200, META);
    engine
        .apply_public_feed(
            NOW,
            serde_json::from_str(r#"{"tickers":[{"symbol":"ETH","last":120,"mark":120}]}"#)
                .unwrap(),
        )
        .unwrap();
    assert_eq!(engine.store().ticker("ETH").unwrap().mark, 120.);
    assert!(!engine.store().loaded().positions);
    assert!(!engine.store().loaded().balance);
}
