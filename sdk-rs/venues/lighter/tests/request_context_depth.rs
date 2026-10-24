//! The native public snapshot waits for managed release and owns no command result.
mod support;
use safe_cex_engine::model::Interval;
use safe_cex_engine::request_context::{
    Claim, Error, HttpAction, Preparation, Prepared, RoutingIntent,
};
use safe_cex_engine::{
    Action, Command, Engine, Event, HttpRequest, Notice, SocketId, TimerId, VenueError,
};
use safe_cex_venue_lighter::{Lighter, Options};
use support::{META, NOW, reply_at};

const DEPTH: &str = r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"2"}],"asks":[{"price":"101","remaining_base_amount":"3"}]}"#;
fn boot(robinhood: bool, testnet: bool) -> (Engine<Lighter>, TimerId, SocketId) {
    let mut engine = safe_cex_engine::test_support::seeded_engine(Lighter::new(Options {
        public_data_only: true,
        robinhood,
        testnet,
        ..Options::default()
    }));
    engine.start(NOW);
    let metadata = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    reply_at(&mut engine, NOW, &metadata, 200, META);
    let actions: Vec<_> = engine.drain_actions().collect();
    let poll = actions
        .iter()
        .find_map(|action| match action {
            Action::SetTimer { timer, at_ms } if *at_ms == NOW + 2000. => Some(*timer),
            _ => None,
        })
        .unwrap();
    let socket = actions
        .iter()
        .find_map(|action| match action {
            Action::WsConnect { socket, .. } => Some(*socket),
            _ => None,
        })
        .unwrap();
    engine.take_frame(NOW);
    (engine, poll, socket)
}
fn open_with_ticker(engine: &mut Engine<Lighter>, socket: SocketId) {
    engine.handle(NOW, Event::WsOpened { socket });
    for action in safe_cex_engine::test_support::prepared_actions(engine, NOW) {
        if let Action::Http(request) = action
            && request.url.ends_with("exchangeStats")
        {
            reply_at(
                engine,
                NOW,
                &request,
                200,
                r#"{"code":200,"order_book_stats":[{"market_id":1,"mark_price":"100"}]}"#,
            );
        }
    }
    engine.drain_actions().for_each(drop);
    engine.take_frame(NOW);
}
fn subscribe(engine: &mut Engine<Lighter>, now: f64, subscribe: bool) -> Vec<Action> {
    engine.handle(
        now,
        Event::Command(if subscribe {
            Command::SubscribeOrderBook {
                symbol: "ETH".into(),
            }
        } else {
            Command::UnsubscribeOrderBook {
                symbol: "ETH".into(),
            }
        }),
    );
    engine.drain_actions().collect()
}
fn preparation(actions: &[Action], depth: bool) -> (Preparation, HttpRequest) {
    assert!(
        !actions
            .iter()
            .any(|action| matches!(action, Action::Http(r) if r.url.contains("orderBookOrders"))),
        "depth must not bypass managed preparation"
    );
    actions
        .iter()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match action.as_ref() {
                HttpAction::Prepare {
                    preparation,
                    request,
                    intent,
                } if request.url.contains(if depth {
                    "orderBookOrders"
                } else {
                    "/candles?"
                }) =>
                {
                    assert_eq!(*intent, RoutingIntent::Public);
                    Some((*preparation, request.clone()))
                }
                _ => None,
            },
            _ => None,
        })
        .expect("actual read must await route preparation")
}
fn release(engine: &mut Engine<Lighter>, token: Preparation, now: f64) -> HttpRequest {
    engine.http_prepared(
        now,
        token,
        Prepared {
            id: token.attempt().request.0,
            expires_at: None,
        },
    );
    let (permit, request) = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::ManagedHttp(action) => match *action {
                HttpAction::Bind {
                    permit, request, ..
                } => Some((permit, request)),
                _ => None,
            },
            _ => None,
        })
        .expect("prepared read binds before claim");
    assert!(matches!(
        engine.claim_http(now, permit),
        Ok(Claim::Released(_))
    ));
    request
}
fn no_terminal(actions: &[Action]) {
    assert!(
        !actions.iter().any(|action| matches!(
            action,
            Action::Notice(Notice::CommandDone { .. } | Notice::CommandFailed { .. })
        )),
        "background depth cannot complete an unrelated command"
    );
}
fn peer(engine: &mut Engine<Lighter>, owner: u32, now: f64) -> Preparation {
    engine.handle(
        now,
        Event::Command(Command::FetchCandles {
            command: owner,
            symbol: "ETH".into(),
            interval: Interval::M1,
            limit: Some(1),
            from_ms: None,
            to_ms: None,
        }),
    );
    preparation(&engine.drain_actions().collect::<Vec<_>>(), false).0
}
#[test]
fn aliases_and_networks_park_exact_native_public_get_until_final_claim() {
    for robinhood in [false, true] {
        for testnet in [false, true] {
            let (mut engine, _, _) = boot(robinhood, testnet);
            let actions = subscribe(&mut engine, NOW, true);
            let (token, unsigned) = preparation(&actions, true);
            let host = match (robinhood, testnet) {
                (false, false) => "https://mainnet.zklighter.elliot.ai",
                (false, true) => "https://testnet-api.lighter.xyz",
                (true, false) => "https://api.rh.lighter.xyz",
                (true, true) => "https://api.rh-testnet.lighter.xyz",
            };
            assert_eq!(unsigned.method, safe_cex_engine::Method::Get);
            assert_eq!(
                unsigned.url,
                format!("{host}/api/v1/orderBookOrders?market_id=1&limit=100")
            );
            assert!(unsigned.headers.is_empty());
            assert!(unsigned.body.is_none());
            assert!(unsigned.signer_headers.is_empty());
            reply_at(&mut engine, NOW + 1., &unsigned, 200, DEPTH);
            no_terminal(&engine.drain_actions().collect::<Vec<_>>());
            assert!(engine.book("ETH").unwrap().is_empty());
            let request = release(&mut engine, token, NOW + 2.);
            assert_eq!(request.url, unsigned.url);
            assert_eq!(request.headers, unsigned.headers);
            reply_at(&mut engine, NOW + 3., &request, 200, DEPTH);
            no_terminal(&engine.drain_actions().collect::<Vec<_>>());
            assert_eq!(engine.book("ETH").unwrap().bids(), &[(100., 2.)]);
            assert_eq!(engine.book("ETH").unwrap().asks(), &[(101., 3.)]);
            engine.take_frame(NOW + 3.);
            reply_at(&mut engine, NOW + 4., &request, 200, DEPTH);
            no_terminal(&engine.drain_actions().collect::<Vec<_>>());
            assert!(engine.take_frame(NOW + 4.).is_none());
        }
    }
}
#[test]
fn original_deadline_covers_parking_and_release_then_native_poll_can_recover() {
    for claimed in [false, true] {
        let (mut engine, poll, _) = boot(false, false);
        let actions = subscribe(&mut engine, NOW, true);
        let (token, unsigned) = preparation(&actions, true);
        let deadline = actions
            .iter()
            .find_map(|action| match action {
                Action::SetTimer { timer, at_ms } if *at_ms == NOW + 20000. => Some(*timer),
                _ => None,
            })
            .unwrap();
        if claimed {
            release(&mut engine, token, NOW + 19000.);
        }
        engine.handle(NOW + 20000., Event::TimerFired { timer: deadline });
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert!(engine.book("ETH").unwrap().is_empty());
        reply_at(&mut engine, NOW + 20001., &unsigned, 200, DEPTH);
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert!(engine.book("ETH").unwrap().is_empty());
        engine.handle(NOW + 22001., Event::TimerFired { timer: poll });
        let (next, _) = preparation(&engine.drain_actions().collect::<Vec<_>>(), true);
        let request = release(&mut engine, next, NOW + 22002.);
        reply_at(&mut engine, NOW + 22003., &request, 200, DEPTH);
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert_eq!(engine.book("ETH").unwrap().bids(), &[(100., 2.)]);
    }
}
#[test]
fn preparation_transport_and_malformed_depth_release_only_background_latch() {
    for failure in 0..3 {
        let (mut engine, poll, _) = boot(false, false);
        let (token, _) = preparation(&subscribe(&mut engine, NOW, true), true);
        if failure == 0 {
            engine.http_preparation_failed(
                NOW + 1.,
                token,
                Error::State,
                VenueError::new("public depth route denied"),
            );
        } else {
            let request = release(&mut engine, token, NOW + 1.);
            if failure == 1 {
                engine.handle(
                    NOW + 2.,
                    Event::HttpFailed {
                        request: request.id,
                        error: "released depth loss",
                    },
                );
            } else {
                reply_at(&mut engine, NOW + 2., &request, 200, "{}");
            }
        }
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert!(engine.book("ETH").unwrap().is_empty());
        engine.handle(NOW + 4000., Event::TimerFired { timer: poll });
        preparation(&engine.drain_actions().collect::<Vec<_>>(), true);
    }
}
#[test]
fn old_success_and_failure_cannot_release_a_resubscribed_depth_owner() {
    for failure in [false, true] {
        let (mut engine, poll, _) = boot(false, false);
        let (old, _) = preparation(&subscribe(&mut engine, NOW, true), true);
        let old = release(&mut engine, old, NOW + 1.);
        subscribe(&mut engine, NOW + 2., false);
        let (replacement, _) = preparation(&subscribe(&mut engine, NOW + 3., true), true);
        if failure {
            engine.handle(
                NOW + 4.,
                Event::HttpFailed {
                    request: old.id,
                    error: "old subscription failed",
                },
            );
        } else {
            reply_at(&mut engine, NOW + 4., &old, 200, DEPTH);
        }
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert!(engine.book("ETH").unwrap().is_empty());
        engine.handle(NOW + 4000., Event::TimerFired { timer: poll });
        assert!(!engine.drain_actions().any(|action| matches!(action,Action::ManagedHttp(action) if matches!(action.as_ref(),HttpAction::Prepare { request,.. } if request.url.contains("orderBookOrders")))));
        let request = release(&mut engine, replacement, NOW + 4001.);
        reply_at(&mut engine, NOW + 4002., &request, 200, DEPTH);
        assert_eq!(engine.book("ETH").unwrap().bids(), &[(100., 2.)]);
    }
}
#[test]
fn newer_ws_revision_wins_and_stale_depth_context_does_not_leak_capacity() {
    let (mut engine, _, socket) = boot(false, false);
    open_with_ticker(&mut engine, socket);
    let (token, _) = preparation(&subscribe(&mut engine, NOW, true), true);
    let request = release(&mut engine, token, NOW + 1.);
    engine.handle(NOW+2.,Event::WsMessage { socket,text:r#"{"channel":"order_book:1","type":"subscribed/order_book","order_book":{"nonce":10,"bids":[{"price":"99","size":"7"}],"asks":[]}}"# });
    reply_at(&mut engine, NOW + 3., &request, 200, DEPTH);
    no_terminal(&engine.drain_actions().collect::<Vec<_>>());
    assert_eq!(engine.book("ETH").unwrap().bids(), &[(99., 7.)]);
    for owner in 1..=256 {
        peer(&mut engine, owner, NOW + 4.);
    }
}
#[test]
fn full_capacity_allocates_no_depth_and_freed_slot_recovers_without_peer_damage() {
    let (mut engine, poll, socket) = boot(false, false);
    open_with_ticker(&mut engine, socket);
    let mut first = None;
    for owner in 1..=256 {
        let token = peer(&mut engine, owner, NOW);
        first.get_or_insert(token);
    }
    let actions = subscribe(&mut engine, NOW, true);
    no_terminal(&actions);
    assert!(!actions.iter().any(|action| matches!(
        action,
        Action::Http(_) | Action::ManagedHttp(_) | Action::SetTimer { .. }
    )));
    engine.http_preparation_failed(
        NOW + 1.,
        first.unwrap(),
        Error::State,
        VenueError::new("free command slot"),
    );
    let actions: Vec<_> = engine.drain_actions().collect();
    assert!(matches!(
        actions
            .iter()
            .find(|action| matches!(action, Action::Notice(Notice::CommandFailed { .. }))),
        Some(Action::Notice(Notice::CommandFailed { command: 1, .. }))
    ));
    engine.handle(NOW + 2001., Event::TimerFired { timer: poll });
    preparation(&engine.drain_actions().collect::<Vec<_>>(), true);
}
#[test]
fn repeated_successful_subscription_seeds_retire_more_than_the_context_bound() {
    let (mut engine, _, _) = boot(false, false);
    for index in 0..300 {
        let now = NOW + f64::from(index) * 3.;
        let (token, _) = preparation(&subscribe(&mut engine, now, true), true);
        let request = release(&mut engine, token, now + 1.);
        reply_at(&mut engine, now + 2., &request, 200, DEPTH);
        no_terminal(&engine.drain_actions().collect::<Vec<_>>());
        assert_eq!(engine.book("ETH").unwrap().bids(), &[(100., 2.)]);
        subscribe(&mut engine, now + 2., false);
    }
}
