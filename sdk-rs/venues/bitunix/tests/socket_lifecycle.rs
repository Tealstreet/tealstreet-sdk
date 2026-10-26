mod support;
use safe_cex_engine::{Action, Command, Engine, Event, Response, SocketId, TimerId};
use safe_cex_venue_bitunix::BitunixFutures;
type TestEngine = Engine<BitunixFutures>;

fn ready() -> (TestEngine, Vec<(String, SocketId)>) {
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..support::Options::default()
    });
    e.start(0.0);
    let r = safe_cex_engine::test_support::prepared_actions(&mut e, 0.)
        .into_iter()
        .find_map(|a| match a {
            Action::Http(r) => Some(r),
            _ => None,
        })
        .unwrap();
    let rows = (0..301).map(|i| format!(r#"{{"symbol":"T{i}USDT","base":"T{i}","quote":"USDT","symbolStatus":"OPEN","basePrecision":3,"quotePrecision":1,"maxLimitOrderVolume":"100"}}"#)).collect::<Vec<_>>().join(",");
    e.handle(
        0.0,
        Event::HttpResponse {
            request: r.id,
            response: Response {
                status: 200,
                headers: "",
                body: &format!(r#"{{"code":0,"data":[{rows}]}}"#),
            },
        },
    );
    let mut sockets = Vec::new();
    loop {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, 0.);
        if actions.is_empty() {
            break;
        }
        for a in actions {
            match a {
                Action::WsConnect {
                    socket,
                    context: Some(context),
                    ..
                } => {
                    sockets.push((context.feed, socket));
                    e.handle(0.0, Event::WsOpened { socket });
                }
                Action::Http(r) if r.url.ends_with("/tickers") => {
                    e.handle(0.0, Event::HttpResponse { request: r.id, response: Response { status: 200, headers: "", body: r#"{"code":0,"data":[{"symbol":"T0USDT","last":"1","open":"1","markPrice":"1","baseVol":"1","quoteVol":"1"}]}"# } });
                }
                _ => {}
            }
        }
    }
    assert_eq!(sockets.len(), 6);
    (e, sockets)
}
fn retry(e: &mut TestEngine, socket: SocketId, at: f64) -> (TimerId, f64) {
    e.handle(at, Event::WsClosed { socket, code: 1006 });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. })),
        "a refused feed must wait for bounded backoff"
    );
    let timers: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect();
    assert_eq!(timers.len(), 1, "exactly one feed retry");
    (timers[0].0, timers[0].1 - at)
}
fn reconnect(e: &mut TestEngine, timer: TimerId, at: f64, feed: &str, recover: bool) -> SocketId {
    e.handle(at, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let sockets: Vec<_> = actions
        .iter()
        .filter_map(|a| match a {
            Action::WsConnect {
                socket,
                context: Some(context),
                ..
            } => {
                assert_eq!(context.feed, feed);
                assert_eq!(context.recover, recover);
                Some(*socket)
            }
            _ => None,
        })
        .collect();
    assert_eq!(sockets.len(), 1);
    sockets[0]
}
fn timers(actions: &[Action]) -> Vec<(TimerId, f64)> {
    actions
        .iter()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .collect()
}

#[test]
fn each_feed_keeps_capped_retry_state_until_actual_stable_traffic() {
    let (mut e, sockets) = ready();
    let mut now = 1.0;
    for (feed, mut socket) in sockets {
        for attempt in 0..8 {
            let nominal = (1000.0 * 2f64.powi(attempt)).min(30_000.0);
            let (timer, delay) = retry(&mut e, socket, now);
            assert!(
                (nominal * 0.75..=nominal).contains(&delay),
                "{feed} attempt{attempt}: {delay}"
            );
            // Duplicate close/open/message and the retired retry cannot reconnect or reset.
            e.handle(now, Event::WsClosed { socket, code: 1006 });
            e.handle(now, Event::WsOpened { socket });
            e.handle(
                now,
                Event::WsMessage {
                    socket,
                    text: r#"{"op":"pong"}"#,
                },
            );
            assert!(e.drain_actions().next().is_none());
            now += delay;
            socket = reconnect(&mut e, timer, now, &feed, false);
            e.handle(now, Event::TimerFired { timer });
            assert!(e.drain_actions().next().is_none());
            // Opening alone must not reset the failed-feed counter.
            e.handle(now, Event::WsOpened { socket });
            e.drain_actions().for_each(drop);
            now += 1.0;
        }
        // Real traffic after a stable epoch resets only this feed.
        now += 31_000.0;
        e.handle(
            now,
            Event::WsMessage {
                socket,
                text: r#"{"op":"pong"}"#,
            },
        );
        e.drain_actions().for_each(drop);
        let (_, delay) = retry(&mut e, socket, now);
        assert!(
            (750.0..=1000.0).contains(&delay),
            "stable {feed} reset: {delay}"
        );
        now += 1.0;
    }
}

#[test]
fn disconnected_futures_clear_consumers_before_retry_and_ignore_old_epochs() {
    use safe_cex_engine::wire::{section, sections};
    let (mut e, sockets) = ready();
    let socket = sockets.iter().find(|(f, _)| f == "futures").unwrap().1;
    e.handle(
        0.0,
        Event::Command(Command::SubscribeOrderBook {
            symbol: "T0USDT".into(),
        }),
    );
    e.drain_actions().for_each(drop);
    e.take_frame(0.0);
    let depth =
        r#"{"channel":"market_t0usdt_depthop_0.1","data":{"bids":[["1","2"]],"asks":[["2","3"]]}}"#;
    e.handle(
        1.0,
        Event::WsMessage {
            socket,
            text: depth,
        },
    );
    assert_eq!(e.book("T0USDT").unwrap().bids(), [(1.0, 2.0)]);
    e.take_frame(1.0);
    e.handle(2.0, Event::WsClosed { socket, code: 1006 });
    assert!(
        e.book("T0USDT").is_none(),
        "disconnect must invalidate immediately"
    );
    let frame = e
        .take_market_frame(2.0)
        .expect("disconnect clears consumer before reopen");
    let clear = sections(frame)
        .find(|(k, _)| *k == section::BOOK)
        .unwrap()
        .1;
    assert_eq!(clear.len(), 16);
    assert_eq!(&clear[4..12], &[0; 8]);
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. }))
    );
    let (timer, deadline) = timers(&actions)[0];
    let delay = deadline - 2.0;
    for event in [
        Event::WsMessage {
            socket,
            text: depth,
        },
        Event::WsOpened { socket },
        Event::WsClosed { socket, code: 1006 },
    ] {
        e.handle(3.0, event);
        assert!(e.book("T0USDT").is_none());
        assert!(e.drain_actions().next().is_none());
    }
    let fresh = reconnect(&mut e, timer, 2.0 + delay, "futures", false);
    e.handle(2.0 + delay, Event::WsOpened { socket: fresh });
    let actions: Vec<_> = e.drain_actions().collect();
    assert_eq!(actions.iter().filter(|a| matches!(a, Action::WsSend { text, .. } if text.contains("market_t0usdt_depthop_0.1"))).count(), 1);
    assert!(e.book("T0USDT").is_none());
    e.handle(3.0 + delay, Event::WsOpened { socket: fresh });
    assert!(
        e.drain_actions().next().is_none(),
        "duplicate open cannot replay twice"
    );
    e.handle(
        4.0 + delay,
        Event::WsMessage {
            socket,
            text: depth,
        },
    );
    assert!(e.book("T0USDT").is_none());
    e.handle(5.0 + delay, Event::WsMessage { socket: fresh, text: r#"{"channel":"market_t0usdt_depthop_0.1","data":{"bids":[],"asks":[["3","4"]]}}"# });
    assert!(e.book("T0USDT").unwrap().bids().is_empty());
    assert_eq!(e.book("T0USDT").unwrap().asks(), [(3.0, 4.0)]);
}

fn reopen(e: &mut TestEngine, socket: SocketId, feed: &str, now: &mut f64) -> (SocketId, TimerId) {
    let (timer, delay) = retry(e, socket, *now);
    *now += delay;
    let fresh = reconnect(e, timer, *now, feed, false);
    e.handle(*now, Event::WsOpened { socket: fresh });
    let actions: Vec<_> = e.drain_actions().collect();
    let watch = timers(&actions)
        .into_iter()
        .find(|(_, at)| *at == *now + 5000.0)
        .unwrap()
        .0;
    (fresh, watch)
}
fn fire_watch(e: &mut TestEngine, timer: TimerId, now: f64) -> (TimerId, Vec<Action>) {
    e.handle(now, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    let next = timers(&actions)[0].0;
    (next, actions)
}

#[test]
fn private_first_pong_gate_preserves_silence_but_expires_from_actual_traffic() {
    let (mut e, sockets) = ready();
    let original = sockets.iter().find(|(f, _)| f == "private").unwrap().1;
    let mut now = 1.0;
    let (socket, mut watch) = reopen(&mut e, original, "private", &mut now);
    let opened = now;
    let mut pings = 0;
    // More than 45 seconds without any pong must remain compatible and ping.
    for elapsed in (5000..=60_000).step_by(5000) {
        let (next, actions) = fire_watch(&mut e, watch, opened + elapsed as f64);
        watch = next;
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { .. } | Action::WsConnect { .. }))
        );
        pings += actions.iter().filter(|a| matches!(a, Action::WsSend { socket: s, text } if *s == socket && text == r#"{"op":"ping"}"#)).count();
    }
    assert_eq!(pings, 4);
    e.handle(
        opened + 60_000.0,
        Event::WsMessage {
            socket,
            text: r#"{"op":"pong"}"#,
        },
    );
    e.drain_actions().for_each(drop);
    for elapsed in (65_000..105_000).step_by(5000) {
        let (next, actions) = fire_watch(&mut e, watch, opened + elapsed as f64);
        watch = next;
        assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    }
    let (timer, actions) = fire_watch(&mut e, watch, opened + 105_000.0);
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    assert!(
        !actions
            .iter()
            .any(|a| matches!(a, Action::WsConnect { .. } | Action::WsSend { .. }))
    );
    let deadline = timers(&actions)[0].1;
    let fresh = reconnect(&mut e, timer, deadline, "private", true);
    e.handle(deadline, Event::WsOpened { socket: fresh });
    let actions: Vec<_> = e.drain_actions().collect();
    watch = timers(&actions)[0].0;
    // A replacement epoch resets the pong gate without resetting retry state.
    let (_, actions) = fire_watch(&mut e, watch, deadline + 60_000.0);
    assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    assert!(actions.iter().any(|a| matches!(a, Action::WsSend { socket: s, text } if *s == fresh && text == r#"{"op":"ping"}"#)));
}

#[test]
fn connecting_timeout_retries_once_with_recover_context_for_every_feed() {
    let (mut e, sockets) = ready();
    let mut now = 1.0;
    for (feed, socket) in sockets {
        let (retry_timer, delay) = retry(&mut e, socket, now);
        now += delay;
        e.handle(now, Event::TimerFired { timer: retry_timer });
        let actions: Vec<_> = e.drain_actions().collect();
        let connecting = actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        let watch = timers(&actions)[0].0;
        e.handle(now + 10_000.0, Event::TimerFired { timer: watch });
        let actions: Vec<_> = e.drain_actions().collect();
        assert!(
            actions
                .iter()
                .any(|a| matches!(a, Action::WsClose { socket } if *socket == connecting))
        );
        assert!(
            !actions
                .iter()
                .any(|a| matches!(a, Action::WsConnect { .. }))
        );
        let (timer, at) = timers(&actions)[0];
        assert!((1500.0..=2000.0).contains(&(at - now - 10_000.0)));
        e.handle(now + 10_001.0, Event::WsOpened { socket: connecting });
        e.handle(
            now + 10_001.0,
            Event::WsClosed {
                socket: connecting,
                code: 1006,
            },
        );
        assert!(e.drain_actions().next().is_none());
        reconnect(&mut e, timer, at, &feed, true);
        now = at + 1.0;
    }
}

#[test]
fn retired_queue_and_login_timers_cannot_send_on_a_replacement_epoch() {
    let (mut e, sockets) = ready();
    let mut now = 1.0;
    let private = sockets.iter().find(|(f, _)| f == "private").unwrap().1;
    e.handle(
        now,
        Event::WsMessage {
            socket: private,
            text: r#"{"op":"login","data":{"result":false}}"#,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    let stale_login = timers(&actions)[0].0;
    let (fresh, watch) = reopen(&mut e, private, "private", &mut now);
    e.handle(now + 2000.0, Event::TimerFired { timer: stale_login });
    assert!(e.drain_actions().next().is_none());
    e.handle(now + 2000.0, Event::WsOpened { socket: fresh });
    assert!(e.drain_actions().next().is_none());
    // Retired supervisor watch and login events cannot affect an active sibling.
    retry(&mut e, fresh, now + 2001.0);
    e.handle(now + 6000.0, Event::TimerFired { timer: watch });
    assert!(e.drain_actions().next().is_none());
    now += 10_000.0;
    let shard = sockets.iter().find(|(f, _)| f == "ticker:0").unwrap().1;
    let (timer, delay) = retry(&mut e, shard, now);
    now += delay;
    let fresh = reconnect(&mut e, timer, now, "ticker:0", false);
    e.handle(now, Event::WsOpened { socket: fresh });
    let actions: Vec<_> = e.drain_actions().collect();
    let stale_queue = timers(&actions)
        .into_iter()
        .find(|(_, at)| *at == now + 250.0)
        .unwrap()
        .0;
    retry(&mut e, fresh, now + 1.0);
    e.handle(now + 251.0, Event::TimerFired { timer: stale_queue });
    assert!(
        e.drain_actions().next().is_none(),
        "queued subscriptions from a retired shard must not leak"
    );
}

#[test]
fn shard_retirement_does_not_clear_books_and_reconnect_replays_current_interests_once() {
    use safe_cex_engine::model::Interval;
    let (mut e, sockets) = ready();
    let futures = sockets.iter().find(|(f, _)| f == "futures").unwrap().1;
    let shard = sockets.iter().find(|(f, _)| f == "price:300").unwrap().1;
    for command in [
        Command::SubscribeOrderBook {
            symbol: "T0USDT".into(),
        },
        Command::SubscribeCandles {
            symbol: "T1USDT".into(),
            interval: Interval::M1,
        },
    ] {
        e.handle(0.0, Event::Command(command));
    }
    e.handle(1.0, Event::WsMessage { socket: futures, text: r#"{"channel":"market_t0usdt_depthop_0.1","data":{"bids":[["1","2"]],"asks":[["2","3"]]}}"# });
    e.drain_actions().for_each(drop);
    retry(&mut e, shard, 2.0);
    assert!(
        e.book("T0USDT").is_some(),
        "unrelated shard cannot invalidate futures books"
    );
    let (timer, delay) = retry(&mut e, futures, 3.0);
    for command in [
        Command::UnsubscribeOrderBook {
            symbol: "T0USDT".into(),
        },
        Command::UnsubscribeCandles {
            symbol: "T1USDT".into(),
            interval: Interval::M1,
        },
        Command::SubscribeOrderBook {
            symbol: "T2USDT".into(),
        },
        Command::SubscribeCandles {
            symbol: "T3USDT".into(),
            interval: Interval::M1,
        },
    ] {
        e.handle(4.0, Event::Command(command));
    }
    assert!(
        !e.drain_actions()
            .any(|a| matches!(a, Action::WsSend { .. }))
    );
    let fresh = reconnect(&mut e, timer, 3.0 + delay, "futures", false);
    e.handle(3.0 + delay, Event::WsOpened { socket: fresh });
    let sent: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::WsSend { text, .. } => Some(text),
            _ => None,
        })
        .collect();
    for topic in ["market_t2usdt_depthop_0.1", "market_t3usdt_klineop_1min"] {
        assert_eq!(
            sent.iter().filter(|s| s.contains(topic)).count(),
            1,
            "{topic}"
        );
    }
    assert!(
        !sent
            .iter()
            .any(|s| s.contains("market_t0usdt") || s.contains("market_t1usdt"))
    );
    e.handle(4.0 + delay, Event::WsOpened { socket: fresh });
    assert!(e.drain_actions().next().is_none());
}
