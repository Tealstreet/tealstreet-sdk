mod support;
use safe_cex_engine::{Action, Event, Level, Notice, TimerId, log::code};
use support::*;

fn message(
    e: &mut TestEngine,
    socket: safe_cex_engine::SocketId,
    at: f64,
    text: &str,
) -> Vec<Action> {
    e.handle(at, Event::WsMessage { socket, text });
    e.drain_actions().collect()
}
fn retry(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .expect("bounded login retry")
}
fn login_sends(actions: &[Action]) -> usize {
    actions
        .iter()
        .filter(|a| matches!(a, Action::WsSend { text, .. } if text.contains("\"login\"")))
        .count()
}
#[test]
fn malformed_login_controls_warn_without_claiming_bad_credentials_and_retry() {
    for text in [
        r#"{"op":"login"}"#,
        r#"{"op":"login","data":null}"#,
        r#"{"op":"login","data":{}}"#,
        r#"{"op":"login","data":{"result":"false","secret":"never-log-me"}}"#,
    ] {
        let (mut e, socket) = startup();
        let actions = message(&mut e, socket, 100.0, text);
        assert!(actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::STREAM_MALFORMED && event.level == Level::Warn)), "{text}");
        assert!(!actions.iter().any(|a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::AUTH_REJECTED || event.message.contains("never-log-me"))));
        let timer = retry(&actions, 2100.0);
        e.handle(2100.0, Event::TimerFired { timer });
        assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 1);
    }
}
#[test]
fn explicit_refusal_remains_visible_but_success_cancels_its_retry() {
    let (mut e, socket) = startup();
    let actions = message(
        &mut e,
        socket,
        100.0,
        r#"{"op":"login","data":{"result":false}}"#,
    );
    assert!(actions.iter().any(
        |a| matches!(a, Action::Notice(Notice::Event(event)) if event.code == code::AUTH_REJECTED)
    ));
    let timer = retry(&actions, 2100.0);
    message(
        &mut e,
        socket,
        200.0,
        r#"{"op":"login","data":{"result":true}}"#,
    );
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 0);
}
#[test]
fn repeated_login_refusals_do_not_postpone_the_existing_retry() {
    let (mut e, socket) = startup();
    let text = r#"{"op":"login","data":{"result":false}}"#;
    let first = message(&mut e, socket, 100.0, text);
    let timer = retry(&first, 2100.0);
    let second = message(&mut e, socket, 2000.0, text);
    assert!(!second.iter().any(|a| matches!(a, Action::SetTimer { .. })));
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 1);
}
#[test]
fn retired_login_retry_cannot_sign_on_a_replacement_socket() {
    let (mut e, socket) = startup();
    let first = message(
        &mut e,
        socket,
        100.0,
        r#"{"op":"login","data":{"result":false}}"#,
    );
    let timer = retry(&first, 2100.0);
    e.handle(200.0, Event::WsClosed { socket, code: 1006 });
    let actions: Vec<_> = e.drain_actions().collect();
    let mut replacement = actions.iter().find_map(|a| match a {
        Action::WsConnect { socket, .. } => Some(*socket),
        _ => None,
    });
    if replacement.is_none() {
        let (retry_timer, at) = actions
            .iter()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .expect("socket retry");
        e.handle(at, Event::TimerFired { timer: retry_timer });
        replacement = e.drain_actions().find_map(|a| match a {
            Action::WsConnect { socket, .. } => Some(socket),
            _ => None,
        });
    }
    e.handle(
        1500.0,
        Event::WsOpened {
            socket: replacement.unwrap(),
        },
    );
    e.drain_actions().for_each(drop);
    e.handle(2100.0, Event::TimerFired { timer });
    assert_eq!(login_sends(&e.drain_actions().collect::<Vec<_>>()), 0);
}
/// Plan control 5: a login that cannot draw signing entropy sends no frame and
/// raises no venue STREAM_UNAVAILABLE; the engine's socket path closes it, and
/// the reconnect re-signs once entropy returns.
#[test]
fn an_entropy_failure_sends_no_login_and_the_reconnect_re_signs() {
    use std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };
    let unavailable = Arc::new(AtomicBool::new(false));
    let toggle = unavailable.clone();
    let mut e = support::engine(support::Options {
        key: Some("replay-key".into()),
        secret: Some("replay-secret".into()),
        ..Default::default()
    });
    e.set_signing_entropy(Box::new(move |bytes| {
        if toggle.load(Ordering::SeqCst) {
            Err("controlled signing unavailable".into())
        } else {
            bytes.fill(7);
            Ok(())
        }
    }))
    .unwrap();
    let (mut e, socket) = startup_with_engine(e);
    unavailable.store(true, Ordering::SeqCst);
    e.handle(100.0, Event::WsClosed { socket, code: 1006 });
    let (mut now, mut connects, mut logins_without, mut logins_with) = (100.0, 0, 0, 0);
    let mut stream_unavailable = false;
    let mut timers: Vec<(TimerId, f64)> = Vec::new();
    for _ in 0..200 {
        let actions = safe_cex_engine::test_support::prepared_actions(&mut e, now);
        for action in &actions {
            match action {
                Action::WsConnect { socket, url, .. } if url.contains("/private/") => {
                    connects += 1;
                    // The second private connect proves the engine closed the
                    // unsigned session itself; entropy returns before it opens.
                    if connects == 2 {
                        unavailable.store(false, Ordering::SeqCst);
                    }
                    e.handle(now, Event::WsOpened { socket: *socket });
                }
                Action::WsClose { socket } => {
                    e.handle(
                        now,
                        Event::WsClosed {
                            socket: *socket,
                            code: 1006,
                        },
                    );
                }
                Action::SetTimer { timer, at_ms } => timers.push((*timer, *at_ms)),
                Action::ClearTimer { timer } => timers.retain(|(t, _)| t != timer),
                Action::Notice(Notice::Event(event)) if event.code == code::STREAM_UNAVAILABLE => {
                    stream_unavailable = true
                }
                _ => {}
            }
        }
        let sends = login_sends(&actions);
        if unavailable.load(Ordering::SeqCst) {
            logins_without += sends;
        } else {
            logins_with += sends;
        }
        if logins_with > 0 {
            break;
        }
        if actions.is_empty() {
            timers.sort_by(|a, b| a.1.total_cmp(&b.1));
            let Some((timer, at)) = (!timers.is_empty()).then(|| timers.remove(0)) else {
                break;
            };
            now = now.max(at);
            e.handle(now, Event::TimerFired { timer });
        }
    }
    assert!(
        connects >= 2,
        "the engine closed the unsigned session and reconnected"
    );
    assert_eq!(logins_without, 0, "nothing signed without entropy");
    assert!(!stream_unavailable, "no venue STREAM_UNAVAILABLE");
    assert_eq!(
        logins_with, 1,
        "the reconnect re-signs once entropy returns"
    );
}
