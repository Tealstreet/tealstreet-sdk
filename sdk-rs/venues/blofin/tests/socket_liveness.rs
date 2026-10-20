#[allow(dead_code)]
mod support;
use safe_cex_engine::model::Balance;
use safe_cex_engine::{Action, Engine, Event, SocketChannel, SocketId, TimerId};
type LivenessEngine = Engine<safe_cex_venue_blofin::Blofin>;
const REJECTED: &str = r#"{"event":"login","code":"60009"}"#;
const ACCEPTED: &str = r#"{"event":"login","code":"0"}"#;
const PONG: &str = r#"pong"#;
fn connecting() -> (LivenessEngine, SocketId, f64, bool) {
    let h = support::Harness::new();
    (h.e, h.private, 0.0, false)
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
