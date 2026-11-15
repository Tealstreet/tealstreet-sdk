mod support;
use safe_cex_engine::model::Balance;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId, TimerId};
type LivenessEngine = Engine<safe_cex_venue_woo::Woo>;
const REJECTED: &str = r#"{"event":"auth","success":false,"message":"bad key"}"#;
const ACCEPTED: &str = r#"{"event":"auth","success":true}"#;
const PONG: &str = r#"{"event":"pong"}"#;

#[test]
fn rejected_login_keeps_only_vendor_code_status_and_closed_reason() {
    let (mut engine, socket, now, _) = started();
    engine.handle(now + 1.0, Event::WsMessage {
        socket,
        text: r#"{"event":"auth","success":false,"code":-1002,"status":401,"error":"IP not allowed for key","message":"invalid secret"}"#,
    });
    let event = engine
        .drain_actions()
        .find_map(|action| match action {
            Action::Notice(safe_cex_engine::Notice::Event(event))
                if event.code == safe_cex_engine::log::code::AUTH_REJECTED =>
            {
                Some(event)
            }
            _ => None,
        })
        .expect("classified auth notice");
    assert!(event.message.contains("WOO code -1002"), "{event:?}");
    assert!(!event.message.contains("Tealstreet IP"));
    assert!(event.fields.iter().any(|(name, value)| *name == "venueCode"
        && *value == safe_cex_engine::log::Value::from("-1002")));
    assert!(event.message.contains("status 401"));
    assert!(event.message.contains("unauthorized: invalid key"));
    let diagnostic = format!("{event:?}");
    assert!(!diagnostic.contains("IP not allowed"));
    assert!(!diagnostic.contains("invalid secret"));
}

#[test]
fn rejected_login_diagnostics_are_bounded_and_do_not_dump_auth_frames() {
    for error in [
        serde_json::json!("permission denied"),
        serde_json::json!({"apikey": "key", "secret": "secret"}),
        serde_json::json!(format!("signature {}", "abcdef0123456789".repeat(4))),
        serde_json::json!("!".repeat(2048)),
    ] {
        let (mut engine, socket, now, _) = started();
        let frame =
            serde_json::json!({"event":"auth", "success":false, "code":"-1002", "error":error,
            "params":{"apikey":"must-never-be-logged", "sign":"must-never-be-logged"}})
            .to_string();
        engine.handle(
            now + 1.0,
            Event::WsMessage {
                socket,
                text: &frame,
            },
        );
        let event = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Notice(safe_cex_engine::Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::AUTH_REJECTED =>
                {
                    Some(event)
                }
                _ => None,
            })
            .expect("error-only refusal is still an auth notice");
        let debug = format!("{event:?}");
        assert!(!debug.contains("must-never-be-logged"));
        assert!(!debug.contains(&"abcdef0123456789".repeat(4)));
        assert!(event.message.contains("WOO code -1002"));
        assert!(!event.message.contains("Tealstreet IP"));
        assert!(!debug.contains("permission denied"));
        for (_, value) in event.fields {
            if let safe_cex_engine::log::Value::Str(text) = value {
                assert!(text.len() <= 512);
            }
        }
    }
}

#[test]
fn unknown_or_missing_auth_code_never_infers_an_ip_refusal_from_vendor_text() {
    for code in [
        serde_json::json!(-9999),
        serde_json::Value::Null,
        serde_json::json!({"key": "private-marker"}),
    ] {
        let (mut engine, socket, now, _) = started();
        let frame = serde_json::json!({"event":"auth", "success":false, "code":code,
            "message":"IP not whitelisted private-marker", "error":"private-marker"})
        .to_string();
        engine.handle(
            now + 1.0,
            Event::WsMessage {
                socket,
                text: &frame,
            },
        );
        let event = engine
            .drain_actions()
            .find_map(|action| match action {
                Action::Notice(safe_cex_engine::Notice::Event(event))
                    if event.code == safe_cex_engine::log::code::AUTH_REJECTED =>
                {
                    Some(event)
                }
                _ => None,
            })
            .expect("auth refusal");
        assert!(event.message.ends_with("unclassified auth refusal"));
        assert!(!event.message.contains("Tealstreet IP"));
        assert!(!format!("{event:?}").contains("private-marker"));
    }
}
// Private startup begins at markets, so its socket may connect before tickers.
fn seed_tickers(e: &mut LivenessEngine) -> Vec<Action> {
    let mut actions = support::prepared_seed_actions(e, 10.);
    let request = actions
        .iter()
        .find_map(|a| match a {
            Action::Http(r) if r.url.contains("/v1/public/futures") => Some(r.clone()),
            _ => None,
        })
        .unwrap();
    support::response(e, &request, 20.0, support::TICKERS);
    actions.extend(e.drain_actions());
    actions
}

fn connecting() -> (LivenessEngine, SocketId, f64, bool) {
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_woo::Woo::new(
        safe_cex_venue_woo::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            application_id: Some("app".into()),
            ..Default::default()
        },
    ));
    e.start(0.0);
    let request = support::prepared_seed_requests(&mut e, 0.).remove(0);
    support::response(&mut e, &request, 10.0, support::MARKETS);
    let socket = seed_tickers(&mut e)
        .into_iter()
        .find_map(|a| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some(socket),
            _ => None,
        })
        .unwrap();
    (e, socket, 20.0, false)
}

fn started() -> (LivenessEngine, SocketId, f64, TimerId) {
    let (mut e, socket, opened_at, already_open) = connecting();
    if !already_open {
        e.drain_actions().for_each(drop);
        e.handle(opened_at, Event::WsOpened { socket });
    }
    let timer = e
        .drain_actions()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if at_ms == opened_at + 5000.0 => Some(timer),
            _ => None,
        })
        .expect("private connection watchdog");
    (e, socket, opened_at, timer)
}

#[test]
fn rejected_login_closes_preserves_state_and_increases_retry_backoff() {
    let (mut e, mut socket, mut now, mut watch) = started();
    e.store_mut().set_balance(Balance {
        total: 42.0,
        ..Default::default()
    });
    for attempt in 0..8 {
        now += 1.0;
        e.handle(
            now,
            Event::WsMessage {
                socket,
                text: REJECTED,
            },
        );
        let actions: Vec<_> = e.drain_actions().collect();
        assert_eq!(
            actions
                .iter()
                .filter(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
                .count(),
            1
        );
        let retries: Vec<_> = actions
            .iter()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .collect();
        assert_eq!(retries.len(), 1);
        let (retry, at) = retries[0];
        let nominal = (1000.0 * 2f64.powi(attempt)).min(30_000.0);
        assert!(
            (nominal * 0.75..=nominal).contains(&(at - now)),
            "attempt {attempt}: {}",
            at - now
        );
        assert_eq!(e.store().balance().total, 42.0);
        e.handle(now, Event::TimerFired { timer: watch });
        e.handle(
            now,
            Event::WsMessage {
                socket,
                text: ACCEPTED,
            },
        );
        e.handle(now, Event::WsClosed { socket, code: 1006 });
        assert!(
            e.drain_actions().next().is_none(),
            "retired socket events must be inert"
        );
        e.handle(at, Event::TimerFired { timer: retry });
        let replacement = e
            .drain_actions()
            .find_map(|a| match a {
                Action::WsConnect {
                    socket,
                    channel: Some(SocketChannel::Private),
                    ..
                } => Some(socket),
                _ => None,
            })
            .expect("private retry");
        assert_ne!(socket, replacement);
        e.handle(
            at,
            Event::WsMessage {
                socket,
                text: REJECTED,
            },
        );
        assert!(e.drain_actions().next().is_none());
        socket = replacement;
        now = at;
        e.handle(now, Event::WsOpened { socket });
        watch = e
            .drain_actions()
            .find_map(|a| match a {
                Action::SetTimer { timer, .. } => Some(timer),
                _ => None,
            })
            .unwrap();
    }
}

#[test]
fn continuous_pongs_cannot_extend_the_authentication_deadline() {
    let (mut e, socket, now, first_watch) = started();
    e.handle(now + 4999.0, Event::WsMessage { socket, text: PONG });
    e.drain_actions().for_each(drop);
    e.handle(now + 5000.0, Event::TimerFired { timer: first_watch });
    let mut timers: Vec<_> = e
        .drain_actions()
        .filter_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
            _ => None,
        })
        .collect();
    let watch = loop {
        let (timer, at) = timers.pop().unwrap();
        assert!(at <= now + 10_000.0, "auth deadline must remain scheduled");
        if at == now + 10_000.0 {
            break timer;
        }
        e.handle(at - 1.0, Event::WsMessage { socket, text: PONG });
        e.drain_actions().for_each(drop);
        e.handle(at, Event::TimerFired { timer });
        timers = e
            .drain_actions()
            .filter_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((timer, at_ms)),
                _ => None,
            })
            .collect();
    };
    e.handle(now + 9999.0, Event::WsMessage { socket, text: PONG });
    e.drain_actions().for_each(drop);
    e.handle(now + 10_000.0, Event::TimerFired { timer: watch });
    assert!(
        e.drain_actions()
            .any(|a| matches!(a, Action::WsClose { socket: s } if s == socket))
    );
}

#[test]
fn accepted_login_cancels_auth_deadline_and_stale_timer_cannot_close_socket() {
    let (mut e, socket, now, old_watch) = started();
    e.handle(
        now + 1.0,
        Event::WsMessage {
            socket,
            text: ACCEPTED,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::ClearTimer { timer } if *timer == old_watch))
    );
    let active_watch = actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(
        now + 2.0,
        Event::WsMessage {
            socket,
            text: ACCEPTED,
        },
    );
    assert!(
        e.drain_actions().next().is_none(),
        "duplicate login acknowledgements are inert"
    );
    e.handle(now + 10_000.0, Event::TimerFired { timer: old_watch });
    assert!(e.drain_actions().next().is_none());
    e.handle(now + 10_001.0, Event::WsMessage { socket, text: PONG });
    e.drain_actions().for_each(drop);
    e.handle(
        now + 10_001.0,
        Event::TimerFired {
            timer: active_watch,
        },
    );
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(!actions.iter().any(|a| matches!(a, Action::WsClose { .. })));
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsSend { socket: s, .. } if *s == socket))
    );
}

#[test]
fn private_connection_that_never_opens_is_closed_and_backed_off() {
    let mut e = safe_cex_engine::test_support::seeded_engine(safe_cex_venue_woo::Woo::new(
        safe_cex_venue_woo::Options {
            key: Some("key".into()),
            secret: Some("secret".into()),
            application_id: Some("app".into()),
            ..Default::default()
        },
    ));
    e.start(0.0);
    let request = support::prepared_seed_requests(&mut e, 0.).remove(0);
    support::response(&mut e, &request, 10.0, support::MARKETS);
    let now = 20.0;
    let actions = seed_tickers(&mut e);
    let (index, socket) = actions
        .iter()
        .enumerate()
        .find_map(|(i, a)| match a {
            Action::WsConnect {
                socket,
                channel: Some(SocketChannel::Private),
                ..
            } => Some((i, *socket)),
            _ => None,
        })
        .unwrap();
    let timer = actions[..index]
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    e.handle(now + 10_000.0, Event::TimerFired { timer });
    let actions: Vec<_> = e.drain_actions().collect();
    assert!(
        actions
            .iter()
            .any(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
    );
    assert!(actions.iter().any(|a| matches!(a, Action::SetTimer { at_ms, .. } if (now + 10_750.0..=now + 11_000.0).contains(at_ms))));
}
