mod support;
use safe_cex_engine::{Action, Event, SocketId, TimerId};
use support::Harness;

fn timer_at(actions: &[Action], at: f64) -> TimerId {
    actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == at => Some(*timer),
            _ => None,
        })
        .unwrap()
}
fn opened_private(h: &mut Harness) -> SocketId {
    let socket = h.socket(true);
    h.actions.clear();
    h.event(Event::WsOpened { socket });
    let ready = timer_at(&h.actions, h.now + 1000.0);
    h.now += 1000.0;
    h.event(Event::TimerFired { timer: ready });
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("public/auth")))
    );
    socket
}
#[test]
fn rejected_login_closes_once_backs_off_and_ignores_retired_auth_ack() {
    let mut h = Harness::new();
    let old = opened_private(&mut h);
    h.actions.clear();
    h.event(Event::WsMessage {
        socket: old,
        text: r#"{"method":"public/auth","code":10002,"message":"UNAUTHORIZED"}"#,
    });
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a,Action::WsClose{socket}if *socket==old))
            .count(),
        1
    );
    let (retry, at) = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
            _ => None,
        })
        .unwrap();
    assert!((750.0..=1000.0).contains(&(at - h.now)));
    h.actions.clear();
    h.event(Event::WsClosed {
        socket: old,
        code: 1008,
    });
    h.event(Event::WsMessage {
        socket: old,
        text: r#"{"method":"public/auth","code":0}"#,
    });
    assert!(h.actions.is_empty());
    h.now = at;
    h.event(Event::TimerFired { timer: retry });
    let replacement = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::WsConnect { socket, url, .. } if url.ends_with("/user") => Some(*socket),
            _ => None,
        })
        .unwrap();
    assert_ne!(replacement, old);
    h.actions.clear();
    h.event(Event::WsOpened {
        socket: replacement,
    });
    let ready = timer_at(&h.actions, h.now + 1000.0);
    h.now += 1000.0;
    h.event(Event::TimerFired { timer: ready });
    h.actions.clear();
    h.event(Event::WsMessage {
        socket: old,
        text: r#"{"method":"public/auth","code":0}"#,
    });
    assert!(h.actions.is_empty());
    for _ in 0..2 {
        h.event(Event::WsMessage {
            socket: replacement,
            text: r#"{"method":"public/auth","code":0}"#,
        });
    }
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a,Action::WsSend{text,..}if text.contains("user.trade")))
            .count(),
        1
    );
}
#[test]
fn heartbeats_cannot_extend_an_unacknowledged_login_deadline() {
    let mut h = Harness::new();
    let start = h.now;
    let initial_watch = timer_at(&h.actions, start + 5000.0);
    let socket = opened_private(&mut h);
    let watch = h
        .actions
        .iter()
        .find_map(|a| match a {
            Action::SetTimer { timer, at_ms } if *at_ms == start + 5000.0 => Some(*timer),
            _ => None,
        })
        .unwrap_or(initial_watch);
    h.actions.clear();
    h.now = start + 4000.0;
    h.event(Event::WsMessage {
        socket,
        text: r#"{"id":41,"method":"public/heartbeat","code":0}"#,
    });
    assert!(h.actions.iter().any(|a|matches!(a,Action::WsSend{text,..}if text.contains("respond-heartbeat")&&text.contains("41"))));
    h.now = start + 5000.0;
    h.event(Event::TimerFired { timer: watch });
    let deadline = timer_at(&h.actions, start + 10_000.0);
    h.now = start + 9000.0;
    h.event(Event::WsMessage {
        socket,
        text: r#"{"id":42,"method":"public/heartbeat","code":0}"#,
    });
    h.actions.clear();
    h.now = start + 10_000.0;
    h.event(Event::TimerFired { timer: deadline });
    assert_eq!(
        h.actions
            .iter()
            .filter(|a| matches!(a,Action::WsClose{socket:s}if *s==socket))
            .count(),
        1
    );
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a,Action::WsSend{text,..}if text.contains("subscribe")))
    );
}
#[test]
fn public_connect_deadline_and_open_flaps_use_the_independent_supervisor() {
    let mut h = Harness::new();
    let public = h.socket(false);
    let private = h.socket(true);
    let connect_index = h
        .actions
        .iter()
        .position(|a| matches!(a,Action::WsConnect{socket,..}if *socket==public))
        .unwrap();
    let watch = h.actions[..connect_index]
        .iter()
        .rev()
        .find_map(|a| match a {
            Action::SetTimer { timer, .. } => Some(*timer),
            _ => None,
        })
        .unwrap();
    h.actions.clear();
    h.now += 10_000.0;
    h.event(Event::TimerFired { timer: watch });
    assert!(
        h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==public))
    );
    assert!(
        !h.actions
            .iter()
            .any(|a| matches!(a,Action::WsClose{socket}if *socket==private))
    );
    for nominal in [1000.0, 2000.0, 4000.0] {
        let (timer, at) = h
            .actions
            .iter()
            .rev()
            .find_map(|a| match a {
                Action::SetTimer { timer, at_ms } => Some((*timer, *at_ms)),
                _ => None,
            })
            .unwrap();
        assert!((nominal * 0.75..=nominal).contains(&(at - h.now)));
        h.actions.clear();
        h.now = at;
        h.event(Event::TimerFired { timer });
        let socket = h
            .actions
            .iter()
            .find_map(|a| match a {
                Action::WsConnect { socket, .. } => Some(*socket),
                _ => None,
            })
            .unwrap();
        h.event(Event::WsOpened { socket });
        h.actions.clear();
        h.event(Event::WsClosed { socket, code: 1006 });
    }
}

#[test]
fn missing_or_malformed_auth_codes_cannot_subscribe_or_cancel_the_login_deadline() {
    for text in [
        r#"{"method":"public/auth"}"#,
        r#"{"method":"public/auth","code":null}"#,
        r#"{"method":"public/auth","code":"0"}"#,
        r#"{"method":"public/auth","code":false}"#,
    ] {
        let mut h = Harness::new();
        let start = h.now;
        let socket = opened_private(&mut h);
        let watch = timer_at(&h.actions, start + 5000.0);
        h.actions.clear();
        h.event(Event::WsMessage { socket, text });
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("subscribe"))),
            "malformed auth must not subscribe: {text}"
        );
        h.now = start + 4000.0;
        h.event(Event::WsMessage {
            socket,
            text: r#"{"method":"public/heartbeat","id":81}"#,
        });
        assert!(h.actions.iter().any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("respond-heartbeat") && text.contains("81"))));
        h.now = start + 5000.0;
        h.event(Event::TimerFired { timer: watch });
        let deadline = timer_at(&h.actions, start + 10_000.0);
        h.now = start + 9000.0;
        h.event(Event::WsMessage { socket, text });
        h.event(Event::WsMessage {
            socket,
            text: r#"{"method":"public/heartbeat","id":82}"#,
        });
        assert!(
            !h.actions
                .iter()
                .any(|a| matches!(a, Action::WsSend { text, .. } if text.contains("subscribe")))
        );
        h.actions.clear();
        h.now = start + 10_000.0;
        h.event(Event::TimerFired { timer: deadline });
        assert_eq!(
            h.actions
                .iter()
                .filter(|a| matches!(a, Action::WsClose { socket: s } if *s == socket))
                .count(),
            1,
            "malformed auth must retain deadline: {text}"
        );
        assert!(h.actions.iter().any(|a| matches!(a, Action::SetTimer { at_ms, .. } if (750.0..=1000.0).contains(&(*at_ms-h.now)))));
    }
}
