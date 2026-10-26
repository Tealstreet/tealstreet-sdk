//! Regressions from the independent public-phase audit.
mod support;
use safe_cex_engine::{Action, Command, Engine, Event, HttpRequest, Response, SocketId, TimerId};
use safe_cex_venue_lighter::Lighter;
use support::Options;
const MARKETS: &str = r#"{"code":200,"order_book_details":[{"market_id":0,"symbol":"ETH","status":"active","market_type":"perp"},{"market_id":1,"symbol":"BTC","status":"active","market_type":"perp"}]}"#;
fn reply(e: &mut Engine<Lighter>, r: &HttpRequest, t: f64, body: &str) {
    e.handle(
        t,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body,
            },
        },
    );
}
fn setup() -> (Engine<Lighter>, SocketId, HttpRequest, TimerId) {
    let mut e = support::engine(Options {
        public_data_only: true,
        ..Options::default()
    });
    e.start(1_000.0);
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 1000.)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap();
    reply(&mut e, &request, 1_000.0, MARKETS);
    let actions = e.drain_actions().collect::<Vec<_>>();
    let socket = actions
        .iter()
        .find_map(|a| {
            if let Action::WsConnect { socket, .. } = a {
                Some(*socket)
            } else {
                None
            }
        })
        .unwrap();
    let poll = actions
        .iter()
        .find_map(|a| {
            if let Action::SetTimer { timer, at_ms } = a {
                (*at_ms == 3_000.0).then_some(*timer)
            } else {
                None
            }
        })
        .unwrap();
    e.handle(1_000.0, Event::WsOpened { socket });
    let seed = safe_cex_engine::test_support::prepared_actions(&mut e, 1_000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.ends_with("exchangeStats").then_some(r)
            } else {
                None
            }
        })
        .unwrap();
    (e, socket, seed, poll)
}
fn message(e: &mut Engine<Lighter>, socket: SocketId, t: f64, text: &str) {
    e.handle(t, Event::WsMessage { socket, text });
}
fn subscribe(e: &mut Engine<Lighter>) -> HttpRequest {
    e.handle(
        1_000.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
    safe_cex_engine::test_support::prepared_actions(e, 1_000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                Some(r)
            } else {
                None
            }
        })
        .unwrap()
}
#[test]
fn nested_market_stats_envelope_updates_the_named_market() {
    let (mut e, socket, _, _) = setup();
    message(
        &mut e,
        socket,
        2_000.0,
        r#"{"type":"update/market_stats","channel":"market_stats:all","data":{"market_stats":{"0":{"market_id":0,"mark_price":"123","current_funding_rate":"0.0012","funding_rate":"0.0008"}}}}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 123.0);
    assert_eq!(e.store().ticker("ETH").unwrap().funding_rate, 0.0001);
}
#[test]
fn ticker_seed_merges_untouched_markets_without_replacing_newer_stream() {
    let (mut e, socket, seed, _) = setup();
    message(
        &mut e,
        socket,
        2_000.0,
        r#"{"channel":"ticker:0","ticker":{"s":"ETH","mark_price":"123"}}"#,
    );
    reply(
        &mut e,
        &seed,
        3_000.0,
        r#"{"code":200,"order_book_stats":[{"market_id":0,"mark_price":"100"},{"market_id":1,"mark_price":"200"}]}"#,
    );
    assert_eq!(e.store().ticker("ETH").unwrap().mark, 123.0);
    assert_eq!(e.store().ticker("BTC").unwrap().mark, 200.0);
}
#[test]
fn malformed_success_depth_does_not_restore_a_stale_book() {
    for invalid in [
        r#"{}"#,
        r#"{"code":200,"message":"missing levels"}"#,
        r#"{"code":200,"order_book":{"code":500,"bids":[],"asks":[]}}"#,
    ] {
        let (mut e, socket, _, poll) = setup();
        let initial = subscribe(&mut e);
        reply(
            &mut e,
            &initial,
            1_000.0,
            r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"2"}],"asks":[{"price":"101","remaining_base_amount":"3"}]}"#,
        );
        message(
            &mut e,
            socket,
            2_000.0,
            r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":10,"bids":[{"price":"100","size":"2"}],"asks":[{"price":"101","size":"3"}]}}"#,
        );
        e.drain_actions().for_each(drop);
        e.handle(100_000.0, Event::TimerFired { timer: poll });
        let request = safe_cex_engine::test_support::prepared_actions(&mut e, 100_000.0)
            .into_iter()
            .find_map(|a| {
                if let Action::Http(r) = a {
                    r.url.contains("orderBookOrders").then_some(r)
                } else {
                    None
                }
            })
            .unwrap();
        reply(&mut e, &request, 100_001.0, invalid);
        // The per-book silence deadline invalidates the old baseline before recovery.
        // A malformed response must not restore it or establish a new snapshot.
        assert!(e.book("ETH").unwrap().is_empty(), "{invalid}");
        message(
            &mut e,
            socket,
            100_002.0,
            r#"{"channel":"order_book:0","type":"update/order_book","order_book":{"begin_nonce":10,"nonce":11,"bids":[{"price":"99","size":"1"}]}}"#,
        );
        assert!(
            e.book("ETH").unwrap().is_empty(),
            "invalid REST cannot establish continuity"
        );
    }
}
#[test]
fn gap_invalidates_rest_request_started_before_the_gap() {
    let (mut e, socket, _, poll) = setup();
    let initial = subscribe(&mut e);
    reply(
        &mut e,
        &initial,
        1_000.0,
        r#"{"code":200,"bids":[],"asks":[]}"#,
    );
    message(
        &mut e,
        socket,
        2_000.0,
        r#"{"channel":"order_book:0","type":"subscribed/order_book","order_book":{"nonce":10,"bids":[{"price":"100","size":"2"}],"asks":[]}}"#,
    );
    e.drain_actions().for_each(drop);
    e.handle(100_000.0, Event::TimerFired { timer: poll });
    let request = safe_cex_engine::test_support::prepared_actions(&mut e, 100_000.0)
        .into_iter()
        .find_map(|a| {
            if let Action::Http(r) = a {
                r.url.contains("orderBookOrders").then_some(r)
            } else {
                None
            }
        })
        .unwrap();
    message(
        &mut e,
        socket,
        100_001.0,
        r#"{"channel":"order_book:0","type":"update/order_book","order_book":{"begin_nonce":9,"nonce":11,"bids":[{"price":"99","size":"1"}],"asks":[]}}"#,
    );
    assert!(e.book("ETH").unwrap().is_empty());
    reply(
        &mut e,
        &request,
        100_002.0,
        r#"{"code":200,"bids":[{"price":"100","remaining_base_amount":"2"}],"asks":[]}"#,
    );
    assert!(
        e.book("ETH").unwrap().is_empty(),
        "pre-gap snapshot cannot establish continuity"
    );
}

#[test]
fn prior_subscription_depth_response_cannot_replace_the_new_seed() {
    let (mut e, _, _, _) = setup();
    let old = subscribe(&mut e);
    e.handle(
        1_001.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
    let new = subscribe(&mut e);
    reply(
        &mut e,
        &old,
        1_002.0,
        r#"{"code":200,"bids":[[90,1]],"asks":[[91,1]]}"#,
    );
    reply(
        &mut e,
        &new,
        1_003.0,
        r#"{"code":200,"bids":[[100,1]],"asks":[[101,1]]}"#,
    );
    assert_eq!(e.book("ETH").unwrap().bids(), [(100.0, 1.0)]);
}

#[test]
fn prior_subscription_depth_failure_cannot_release_the_new_request() {
    let (mut e, _, _, poll) = setup();
    let old = subscribe(&mut e);
    e.handle(
        1_001.0,
        Event::Command(Command::UnsubscribeOrderBook {
            symbol: "ETH".into(),
        }),
    );
    let _new = subscribe(&mut e);
    e.handle(
        1_002.0,
        Event::HttpFailed {
            request: old.id,
            error: "old request failed",
        },
    );
    e.handle(5_000.0, Event::TimerFired { timer: poll });
    assert!(
        !safe_cex_engine::test_support::prepared_actions(&mut e, 5_000.0)
            .into_iter()
            .any(|a| matches!(a, Action::Http(r) if r.url.contains("orderBookOrders")))
    );
}
#[test]
fn wholly_unusable_ticker_seed_retains_live_quotes_and_retries_with_backoff() {
    for invalid in [
        r#"{"market_id":0,"mark_price":"bad"}"#,
        r#"{"market_id":999,"mark_price":"100"}"#,
        r#"{}"#,
    ] {
        let (mut e, socket, mut request, _) = setup();
        message(
            &mut e,
            socket,
            1000.,
            r#"{"channel":"ticker:0","ticker":{"mark_price":"123"}}"#,
        );
        e.drain_actions().for_each(drop);
        let previous = e.store().tickers().to_vec();
        for (at, retry_at) in [(1000., 2000.), (2000., 4000.)] {
            reply(
                &mut e,
                &request,
                at,
                &format!(r#"{{"code":200,"order_book_stats":[{invalid}]}}"#),
            );
            assert_eq!(e.store().tickers(), previous);
            let actions = e.drain_actions().collect::<Vec<_>>();
            assert!(actions.iter().any(|a| matches!(
                a,
                Action::Notice(safe_cex_engine::Notice::Event(safe_cex_engine::LogEvent {
                    level: safe_cex_engine::Level::Warn,
                    ..
                }))
            )));
            let timer = actions
                .iter()
                .find_map(|a| match a {
                    Action::SetTimer { timer, at_ms }
                        if support::jittered(*at_ms - at, retry_at - at) =>
                    {
                        Some(*timer)
                    }
                    _ => None,
                })
                .expect("bad ticker source must back off even while live quotes exist");
            e.handle(retry_at, Event::TimerFired { timer });
            request = safe_cex_engine::test_support::prepared_actions(&mut e, retry_at)
                .into_iter()
                .find_map(|a| match a {
                    Action::Http(r) if r.url.ends_with("exchangeStats") => Some(r),
                    _ => None,
                })
                .expect("retry must send a fresh ticker read");
        }
        reply(
            &mut e,
            &request,
            4000.,
            &format!(
                r#"{{"code":200,"order_book_stats":[{invalid},{{"market_id":1,"mark_price":"200"}}]}}"#
            ),
        );
        assert_eq!(e.store().ticker("ETH").unwrap().mark, 123.);
        assert_eq!(e.store().ticker("BTC").unwrap().mark, 200.);
        assert!(
            !e.drain_actions()
                .any(|a| matches!(a, Action::SetTimer { .. }))
        );
    }
}
