//! Trusted public quotes retain native private position valuation.
mod support;
use safe_cex_engine::{Action, Engine, Event, PublicFeedPolicy, SocketChannel, SocketId, TimerId};
use safe_cex_venue_lighter::Lighter;
use support::Options;
use support::*;

const STATS: &str = r#"{"code":200,"order_book_stats":[{"market_id":1,"last_trade_price":"100","mark_price":"100","index_price":"100"},{"market_id":2,"last_trade_price":"200","mark_price":"200","index_price":"200"}]}"#;
struct Rig {
    engine: Engine<Lighter>,
    public: Option<SocketId>,
    timers: Vec<(TimerId, f64)>,
    reads: Vec<String>,
    sockets: Vec<(String, Option<SocketChannel>)>,
}
impl Rig {
    fn new(policy: PublicFeedPolicy) -> Self {
        Self::with_options(policy, options())
    }
    fn with_options(policy: PublicFeedPolicy, options: Options) -> Self {
        let mut engine = support::engine(options);
        engine.set_public_feed_policy(policy).unwrap();
        engine.start(NOW);
        let mut rig = Self {
            engine,
            public: None,
            timers: Vec::new(),
            reads: Vec::new(),
            sockets: Vec::new(),
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
            let mut actions =
                safe_cex_engine::test_support::prepared_actions(&mut self.engine, now);
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
                        } else if request.url.contains("accountLimits") {
                            assert!(request.headers.iter().any(|(name, value)| {
                                name.eq_ignore_ascii_case("authorization") && !value.is_empty()
                            }));
                            r#"{"code":200,"user_tier_name":"premium"}"#
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
                        socket,
                        channel,
                        url,
                        ..
                    } => {
                        self.sockets.push((url, channel.clone()));
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
fn shared_price_estimates_and_native_account_pnl_keep_their_authority_after_reconciliation() {
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
    // The authenticated ACCOUNT reports PnL0/equity1000; shared quotes
    // estimate displayed position PnL without rewriting native wallet principal.
    assert_eq!(rig.engine.store().balance().upnl, 0.);
    assert_eq!(rig.engine.store().balance().total, 1000.);
    assert_eq!(rig.engine.store().balance().free, 900.);
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
    let mut engine = support::engine(options());
    engine
        .set_public_feed_policy(PublicFeedPolicy::Shared)
        .unwrap();
    engine.start(NOW);
    let request = safe_cex_engine::test_support::prepared_actions(&mut engine, NOW)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
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

#[test]
fn robinhood_non_direct_modes_keep_own_deployment_metadata_private_state_and_shared_valuation() {
    for testnet in [false, true] {
        let host = if testnet {
            "https://api.rh-testnet.lighter.xyz"
        } else {
            "https://api.rh.lighter.xyz"
        };
        let socket_url = format!("{}/stream", host.replace("https:", "wss:"));
        for policy in [PublicFeedPolicy::Disabled, PublicFeedPolicy::Shared] {
            let mut rig = Rig::with_options(
                policy,
                Options {
                    robinhood: true,
                    testnet,
                    ..options()
                },
            );
            assert_eq!(
                rig.sockets,
                vec![(socket_url.clone(), Some(SocketChannel::Private))]
            );
            assert!(rig.public.is_none());
            assert!(rig.reads.iter().all(|url| url.starts_with(host)));
            assert!(rig.reads.iter().any(|url| url.contains("orderBookDetails")));
            assert!(rig.reads.iter().any(|url| url.contains("accountLimits")));
            assert_eq!(rig.engine.store().market("ETH").unwrap().quote, "USDG");
            assert_eq!(rig.engine.store().market("ETH").unwrap().id, "1");
            assert_eq!(rig.engine.store().market("BTC").unwrap().quote, "USDG");
            assert_eq!(rig.engine.store().orders()[0].id, "42");
            assert_eq!(rig.engine.store().balance().free, 900.);
            let metadata = rig.engine.store().markets().to_vec();
            if policy == PublicFeedPolicy::Shared {
                let balance = rig.engine.store().balance().clone();
                rig.shared_quote(120., 120., 120.);
                assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 40.);
                assert_eq!(rig.engine.store().balance(), &balance);
                rig.fire_due(NOW + 6000., &STATS.replace("100", "55"));
                let ticker = rig.engine.store().ticker("ETH").unwrap();
                assert_eq!((ticker.last, ticker.mark, ticker.index), (120., 120., 120.));
                assert_eq!(rig.engine.store().positions()[0].unrealized_pnl, 40.);
                // Native account equity/PnL remains the balance authority.
                assert_eq!(rig.engine.store().balance().upnl, 0.);
                assert_eq!(rig.engine.store().balance().total, 1000.);
                assert_eq!(rig.engine.store().balance().free, 900.);
                assert_eq!(rig.engine.store().markets(), metadata);
                assert_eq!(
                    rig.sockets,
                    vec![(socket_url.clone(), Some(SocketChannel::Private))]
                );
            }
        }
    }
}
